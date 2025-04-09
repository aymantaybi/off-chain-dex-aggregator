use std::{sync::Arc, time::Instant};

use alloy::{
    eips::BlockId,
    network::AnyNetwork,
    primitives::{Bytes, Uint},
    providers::{IpcConnect, Provider, ProviderBuilder},
    sol,
    sol_types::{SolCall, SolEvent, SolValue},
};
use revm::{
    db::{AlloyDB, CacheDB, EmptyDB},
    primitives::{
        address, Account, Address, ExecutionResult, HashMap, ResultAndState, TxKind, U256,
    },
    Database, DatabaseRef, Evm, InMemoryDB,
};

use off_chain_dex_aggregator::{adapters::path::PathAdapter, helpers::build_provider, Aggregator};
use off_chain_dex_aggregator::{adapters::SwapMode, helpers::build_evm};

use dotenvy::dotenv;
use revm_proxy_db::{load_snapshot_from_file, save_snapshot_to_file, NewFetch, Snapshot};
use tokio::sync::mpsc;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use AggregateRouter::execute_1Call;
use ERC20::Transfer;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    ERC20,
    "src/abi/ERC20.json"
);

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    AggregateRouter,
    "src/abi/AggregateRouter.json"
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

#[tokio::main]
async fn main() -> eyre::Result<()> {
    dotenv()?;

    let env_filter = EnvFilter::from_default_env();
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(env_filter)
        .with_line_number(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let provider = build_provider().await?;

    let block_number = provider.get_block_number().await?;

    let mut evm = build_evm(provider.clone()).await?;

    evm.db_mut().db.db.set_block_number(block_number.into());

    let path = "data/db.json".to_string();

    if let Ok(snapshot) = load_snapshot_from_file(path) {
        let Snapshot {
            block_number,
            cache_db,
        } = snapshot;

        let CacheDB {
            accounts,
            contracts,
            logs,
            block_hashes,
            ..
        } = cache_db;

        warn!(
            block_number = block_number,
            accounts = accounts.len(),
            contracts = contracts.len(),
            logs = logs.len(),
            block_hashes = block_hashes.len(),
            "Using cached data from db.json"
        );

        evm.db_mut().accounts = accounts;
        evm.db_mut().contracts = contracts;
        evm.db_mut().logs = logs;
        evm.db_mut().block_hashes = block_hashes;
    };

    let (tx, mut rx) = mpsc::unbounded_channel();

    evm.db_mut().db.sender = tx.into();

    let mut paths = vec![];

    let pool_in_path = PathUniswapV3Pool {
        address: address!("392d372f2a51610e9ac5b741379d5631ca9a1c7f"),
        token_in: address!("0b7007c13325c48911f73a2dad5fa5dcbf808adc"),
        token_out: address!("e514d9deb7966c8be0ca922de8a064264ea6bcd4"),
        fee: Uint::<24, 1>::from(3000),
    };
    let path = SwapPath::KatanaV3(vec![pool_in_path]);

    paths.push(path);

    let pool_in_path = PathUniswapV2Pool {
        address: address!("4f7687affc10857fccd0938ecda0947de7ad3812"),
        token_in: address!("0b7007c13325c48911f73a2dad5fa5dcbf808adc"),
        token_out: address!("e514d9deb7966c8be0ca922de8a064264ea6bcd4"),
    };
    let path = SwapPath::KatanaV2(vec![pool_in_path]);

    paths.push(path);

    let mut aggregator = Aggregator { evm, paths };

    let amount = U256::from(100e6);

    let now = Instant::now();
    let quote = aggregator.quote(amount, SwapMode::In, 10);

    dbg!(&quote);

    let sum: U256 = quote.into_iter().sum();

    dbg!(sum, now.elapsed().as_millis());

    let mut db = InMemoryDB::default();

    let sender = aggregator.evm.db_mut().db.sender.take();

    drop(sender);

    while let Some(new_fetch) = rx.recv().await {
        match new_fetch {
            NewFetch::Basic {
                address,
                account_info,
            } => {
                db.insert_account_info(address, account_info);
                info!("Inserted account info for {address}");
            }
            NewFetch::Storage {
                address,
                index,
                value,
            } => {
                db.insert_account_storage(address, index, value).unwrap();
                info!("Inserted account storage for {address} {index} {value}");
            }
        };
    }

    if !db.accounts.is_empty() {
        let _ = save_snapshot_to_file("data/db.json".to_string(), &db, block_number)?;
    } else {
        warn!("No accounts to save");
    }

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
