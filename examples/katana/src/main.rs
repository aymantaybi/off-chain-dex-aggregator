use alloy::{
    eips::BlockId,
    network::AnyNetwork,
    primitives::{Bytes, Uint},
    providers::{IpcConnect, Provider, ProviderBuilder},
    sol,
    sol_types::{SolCall, SolEvent, SolValue},
};
use dotenvy::dotenv;
use katana::{
    helpers::encode_v3_path,
    types::{PathUniswapV2Pool, PathUniswapV3Pool, SwapPath},
    ERC20::Transfer,
};
use katana::{AggregateRouter::execute_1Call, AGGREGATE_ROUTER_ADDRESS};
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
