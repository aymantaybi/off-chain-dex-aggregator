use crate::AGGREGATE_ROUTER_ADDRESS;
use crate::ERC20::Transfer;
use crate::{helpers::encode_v3_path, AggregateRouter::execute_1Call};
use alloy::{
    eips::BlockId,
    network::AnyNetwork,
    primitives::{Bytes, Uint},
    providers::{IpcConnect, Provider, ProviderBuilder},
    sol,
    sol_types::{SolCall, SolEvent, SolValue},
};
use dotenvy::dotenv;
use revm::{
    db::{AlloyDB, CacheDB, EmptyDB},
    primitives::{
        address, Account, Address, ExecutionResult, HashMap, ResultAndState, TxKind, U256,
    },
    Database, DatabaseRef, Evm, InMemoryDB,
};
use revm_proxy_db::{load_snapshot_from_file, save_snapshot_to_file, NewFetch, Snapshot};
use smart_order_router::{adapters::path::PathAdapter, helpers::build_provider, Aggregator};
use smart_order_router::{adapters::SwapMode, helpers::build_evm};
use std::{sync::Arc, time::Instant};
use tokio::sync::mpsc;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

pub struct PathUniswapV2Pool {
    pub address: Address,
    pub token_in: Address,
    pub token_out: Address,
}
pub struct PathUniswapV3Pool {
    pub address: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub fee: Uint<24, 1>,
}

pub enum SwapPath {
    KatanaV2(Vec<PathUniswapV2Pool>),
    KatanaV3(Vec<PathUniswapV3Pool>),
}

impl PathAdapter for SwapPath {
    fn swap<EXT, DB>(
        &mut self,
        evm: &mut Evm<'_, EXT, DB>,
        amount: U256,
        mode: SwapMode,
    ) -> eyre::Result<(U256, HashMap<Address, Account>)>
    where
        DB: Database + DatabaseRef,
        <DB as revm::Database>::Error: std::error::Error + Sync + Send + 'static,
    {
        let mut token_in = Address::ZERO;
        let mut token_out = Address::ZERO;

        let tx = evm.tx_mut();
        let caller = tx.caller;
        match self {
            SwapPath::KatanaV2(pools) => {
                token_in = pools[0].token_in;
                token_out = pools[pools.len() - 1].token_out;
                let (command, amount_a, amount_b) = match mode {
                    SwapMode::In => (0x08 as u8, amount, U256::ZERO),
                    SwapMode::Out => (0x09 as u8, U256::MAX, amount),
                };
                let mut path = vec![token_in];
                for pool in pools {
                    path.push(pool.token_out);
                }
                let input = (caller, amount_a, amount_b, path, true).abi_encode_params();
                let commands = [command].abi_encode_packed();
                let call = execute_1Call {
                    commands: commands.into(),
                    inputs: vec![input.into()],
                    deadline: U256::from(32509705735_u64),
                };
                tx.transact_to = TxKind::Call(AGGREGATE_ROUTER_ADDRESS);
                tx.data = call.abi_encode().into();
            }
            SwapPath::KatanaV3(pools) => {
                token_in = pools[0].token_in;
                token_out = pools[pools.len() - 1].token_out;
                let (command, amount_a, amount_b) = match mode {
                    SwapMode::In => (0x00 as u8, amount, U256::ZERO),
                    SwapMode::Out => (0x01 as u8, U256::MAX, amount),
                };
                let path = encode_v3_path(&pools);
                let input = (caller, amount_a, amount_b, path, true).abi_encode_params();
                let commands = [command].abi_encode_packed();
                let call = execute_1Call {
                    commands: commands.into(),
                    inputs: vec![input.into()],
                    deadline: U256::from(32509705735_u64),
                };
                tx.transact_to = TxKind::Call(AGGREGATE_ROUTER_ADDRESS);
                tx.data = call.abi_encode().into();
            }
        };

        let ResultAndState { result, state } = evm.transact()?;
        let mut amount_in = U256::ZERO;
        let mut amount_out = U256::ZERO;
        match result {
            ExecutionResult::Success {
                reason,
                gas_used,
                gas_refunded,
                logs,
                output,
            } => {
                for log in logs {
                    if let Ok(decoded) = Transfer::decode_log(&log, false) {
                        if token_in == log.address && decoded.from == caller {
                            amount_in = decoded.value;
                        } else if token_out == log.address && decoded.to == caller {
                            amount_out = decoded.value;
                        };
                    }
                }
            }
            ExecutionResult::Revert { gas_used, output } => todo!(),
            ExecutionResult::Halt { reason, gas_used } => todo!(),
        };
        let amount = match mode {
            SwapMode::In => amount_out,
            SwapMode::Out => amount_in,
        };
        Ok((amount, state))
    }
}
