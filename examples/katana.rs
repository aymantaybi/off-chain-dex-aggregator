use std::sync::Arc;

use alloy::{
    eips::BlockId,
    network::AnyNetwork,
    primitives::{Bytes, Uint},
    providers::{IpcConnect, ProviderBuilder},
    sol,
    sol_types::{SolCall, SolValue},
};
use revm::{
    db::{AlloyDB, CacheDB},
    primitives::{address, Account, Address, HashMap, ResultAndState, TxKind, U256},
    Database, DatabaseRef, Evm,
};

use off_chain_dex_aggregator::adapters::SwapMode;
use off_chain_dex_aggregator::{adapters::path::PathAdapter, Aggregator};
use tracing::info;

sol!(
    #[allow(missing_docs)]
    #[derive(Debug, PartialEq, Eq)]
    function execute(bytes calldata commands, bytes[] calldata inputs, uint256 deadline) external payable;
);

static AGGREGATE_ROUTER_ADDRESS: Address = address!("5f0acdd3ec767514ff1bf7e79949640bf94576bd");

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
        let tx = evm.tx_mut();
        match self {
            SwapPath::KatanaV2(pools) => todo!(),
            SwapPath::KatanaV3(pools) => {
                let (command, amount_a, amount_b) = match mode {
                    SwapMode::In => (0x00 as u8, amount, U256::ZERO),
                    SwapMode::Out => (0x01 as u8, U256::MAX, amount),
                };
                let path = encode_v3_path(&pools);
                let input = (tx.caller, amount_a, amount_b, path, true).abi_encode_params();

                let commands = [command].abi_encode_packed();

                let call = executeCall {
                    commands: commands.into(),
                    inputs: vec![input.into()],
                    deadline: U256::from(32509705735_u64),
                };

                tx.transact_to = TxKind::Call(AGGREGATE_ROUTER_ADDRESS);
                tx.data = call.abi_encode().into();
                let ResultAndState { result, state } = evm.transact()?;
                dbg!(result);
                todo!()
            }
        };
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let ipc = IpcConnect::new("/mnt/blockstorage/node/geth.ipc".to_string());
    let provider = ProviderBuilder::new()
        .network::<AnyNetwork>()
        .on_ipc(ipc)
        .await?;
    let provider = Arc::new(provider);

    info!("Provider connected!");

    let alloy_db = AlloyDB::new(provider.clone(), BlockId::default()).unwrap();
    let cache_db = CacheDB::new(alloy_db);
    let evm = Evm::builder()
        .with_db(cache_db)
        .modify_cfg_env(|cfg| {
            cfg.chain_id = 2020;
        })
        .build();

    let pool_in_path = PathUniswapV2Pool {
        address: address!("4f7687affc10857fccd0938ecda0947de7ad3812"),
        token_in: address!("0b7007c13325c48911f73a2dad5fa5dcbf808adc"),
        token_out: address!("e514d9deb7966c8be0ca922de8a064264ea6bcd4"),
    };

    let path = SwapPath::KatanaV2(vec![pool_in_path]);
    let paths = vec![path];

    let mut aggregator = Aggregator { evm, paths };

    let amount = U256::from(1e6);

    aggregator.quote(amount, SwapMode::In, 10);

    Ok(())
}

pub fn encode_v3_path(pools: &Vec<PathUniswapV3Pool>) -> Bytes {
    let first_pool = pools.first().expect("Cannot encode empty pools path");
    let mut path = Vec::new();
    path.extend_from_slice(first_pool.token_in.as_slice());
    for pool in pools {
        let fee_bytes = pool.fee.to_be_bytes::<3>();
        path.extend_from_slice(&fee_bytes);
        path.extend_from_slice(pool.token_out.as_slice());
    }
    Bytes::from(path)
}
