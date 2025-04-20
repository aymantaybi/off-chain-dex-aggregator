use crate::ERC20::Transfer;
use crate::{helpers::encode_v3_path, AggregateRouter::execute_1Call};
use crate::{AGGREGATE_ROUTER_ADDRESS, KATANA_ROUTER_ADDRESS};
use alloy::dyn_abi::abi::decode;
use alloy::sol;
use alloy::{
    primitives::Uint,
    sol_types::{SolCall, SolEvent, SolValue},
};
use revm::primitives::{EvmState, Output};
use revm::{
    primitives::{Account, Address, ExecutionResult, HashMap, ResultAndState, TxKind, U256},
    Database, DatabaseRef, Evm,
};
use smart_order_router::adapters::path::PathAdapter;
use smart_order_router::adapters::{SwapMode, SwapOutput};

sol! {
    #[derive(Debug)]
    function swapExactTokensForTokens(uint amountIn, uint amountOutMin, address[] calldata path, address to, uint deadline) returns (uint[] memory amounts);
    #[derive(Debug)]
    function swapTokensForExactTokens(uint amountOut, uint amountInMax, address[] calldata path, address to, uint deadline) returns (uint[] memory amounts);
    #[derive(Debug)]
    function swap(address recipient, bool zeroForOne, int256 amountSpecified, uint160 sqrtPriceLimitX96, bytes data ) returns (int256 amount0, int256 amount1);
}

#[derive(Debug, Clone)]
pub struct PathKatanaV2Pool {
    pub address: Address,
    pub token_in: Address,
    pub token_out: Address,
}

#[derive(Debug, Clone)]
pub struct PathKatanaV3Pool {
    pub address: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub fee: Uint<24, 1>,
}

#[derive(Debug, Clone)]
pub enum PathPool {
    KatanaV2(PathKatanaV2Pool),
    KatanaV3(PathKatanaV3Pool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolVariant {
    KatanaV2,
    KatanaV3,
}

impl PathPool {
    pub fn token_in(&self) -> Address {
        match self {
            PathPool::KatanaV2(pool) => pool.token_in,
            PathPool::KatanaV3(pool) => pool.token_in,
        }
    }

    pub fn token_out(&self) -> Address {
        match self {
            PathPool::KatanaV2(pool) => pool.token_out,
            PathPool::KatanaV3(pool) => pool.token_out,
        }
    }

    pub fn variant(&self) -> PoolVariant {
        match self {
            PathPool::KatanaV2(_) => PoolVariant::KatanaV2,
            PathPool::KatanaV3(_) => PoolVariant::KatanaV3,
        }
    }
}

pub type PoolsPath = Vec<PathPool>;

#[derive(Debug, Clone)]
pub struct SplitPath {
    pools: PoolsPath,
    variant: PoolVariant,
}

#[derive(Debug, Clone)]
pub struct Path {
    pub pools: PoolsPath,
    pub splits: Vec<SplitPath>,
}

impl Path {
    pub fn new(pools: PoolsPath) -> Self {
        let splits = Self::split(pools.clone());
        Self { pools, splits }
    }

    /// Splits the current `Path` into a vector of sub-paths, where each sub-path
    /// contains pools of the same variant. The sub-paths are returned in the order
    /// they are found.
    ///
    /// # Returns
    /// A `Vec<Path>` where each `Path` represents a contiguous sequence of pools
    /// with the same variant.
    ///
    /// # Example
    /// Given a `Path` with pools of mixed variants, this method will group pools
    /// with the same variant into separate `Path` objects and return them as a
    /// vector.
    ///
    /// # Notes
    /// - If the `Path` contains no pools, an empty vector is returned.
    /// - The `variant` of each sub-path is determined by the `variant` of the pools
    ///   it contains.
    fn split(pools: PoolsPath) -> Vec<SplitPath> {
        let mut paths = vec![];
        let mut iterator = pools.into_iter();
        let Some(pool) = iterator.next() else {
            return paths;
        };
        let variant = pool.variant().into();
        let pools = vec![pool];
        let path = SplitPath { variant, pools };
        paths.push(path);
        while let Some(pool) = iterator.next() {
            let variant = pool.variant().into();
            let last_index = paths.len() - 1;
            let last = &mut paths[last_index];
            if last.variant == variant {
                last.pools.push(pool);
            } else {
                let pools = vec![pool];
                let path = SplitPath { variant, pools };
                paths.push(path);
            }
        }
        paths
    }
}

/// We can get the input / output in one tx if theres only one path. Otherwise the number of tx = number of homogenous paths.
impl PathAdapter for Path {
    fn swap<EXT, DB>(
        &mut self,
        evm: &mut Evm<'_, EXT, DB>,
        amount: U256,
        mode: SwapMode,
    ) -> eyre::Result<SwapOutput>
    where
        DB: Database + DatabaseRef,
        <DB as Database>::Error: std::error::Error + Sync + Send + 'static,
    {
        let mut output = SwapOutput::new(amount);
        // The output of a split path is the input of the next one.
        for split_path in self.splits.iter() {
            let SplitPath { variant, pools } = split_path;
            match variant {
                PoolVariant::KatanaV2 => {
                    let SwapOutput { amount, state } = v2_swap(evm, pools, output.amount, mode)?;
                    output.state.extend(state);
                    output.amount = amount;
                }
                PoolVariant::KatanaV3 => todo!(),
            };
        }
        Ok(output)
    }
}

pub fn v2_swap<EXT, DB>(
    evm: &mut Evm<EXT, DB>,
    pools: &Vec<PathPool>,
    amount: U256,
    mode: SwapMode,
) -> eyre::Result<SwapOutput>
where
    DB: Database + DatabaseRef,
    <DB as Database>::Error: std::error::Error + Sync + Send + 'static,
{
    let token_in = pools[0].token_in();
    let mut path = vec![token_in];
    for pool in pools {
        path.push(pool.token_out());
    }
    let data = match mode {
        SwapMode::In => {
            let call = swapExactTokensForTokensCall {
                amountIn: amount,
                amountOutMin: U256::ZERO,
                path,
                to: Address::ZERO,
                deadline: U256::MAX,
            };
            call.abi_encode()
        }
        SwapMode::Out => {
            let call = swapTokensForExactTokensCall {
                amountOut: amount,
                amountInMax: U256::MAX,
                path,
                to: Address::ZERO,
                deadline: U256::MAX,
            };
            call.abi_encode()
        }
    };
    let tx = evm.tx_mut();
    tx.transact_to = TxKind::Call(KATANA_ROUTER_ADDRESS);
    tx.data = data.into();
    let ResultAndState { result, state } = evm.transact()?;
    match result {
        ExecutionResult::Success { output, .. } => match output {
            Output::Call(bytes) => {
                let amounts = Vec::<U256>::abi_decode(&bytes, false)?;
                match mode {
                    SwapMode::In => {
                        let amount = amounts[amounts.len() - 1];
                        let output = SwapOutput { amount, state };
                        return Ok(output);
                    }
                    SwapMode::Out => {
                        let amount = amounts[0];
                        let output = SwapOutput { amount, state };
                        return Ok(output);
                    }
                }
            }
            Output::Create(bytes, address) => todo!(),
        },
        ExecutionResult::Revert { gas_used, output } => todo!(),
        ExecutionResult::Halt { reason, gas_used } => todo!(),
    }
}

mod tests {
    use super::*;
    use alloy::primitives::address;
    use smart_order_router::adapters::SwapMode;

    #[test]
    fn test_split() {
        let pool_path1 = PathPool::KatanaV2(PathKatanaV2Pool {
            address: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
            token_in: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
            token_out: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
        });
        let pool_path2 = PathPool::KatanaV3(PathKatanaV3Pool {
            address: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
            token_in: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
            token_out: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
            fee: Uint::from(3000),
        });
        let pool_path3 = PathPool::KatanaV3(PathKatanaV3Pool {
            address: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
            token_in: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
            token_out: address!("0xcad9e7aa2c3ef07bad0a7b69f97d059d8f36edd2"),
            fee: Uint::from(3000),
        });
        let pools = vec![pool_path1, pool_path2, pool_path3];
        let path = Path::new(pools);
        let split_paths = path.splits;
        dbg!(&split_paths);
    }
}
