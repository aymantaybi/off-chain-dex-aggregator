use crate::ERC20::Transfer;
use crate::{types::PathUniswapV3Pool, AggregateRouter::execute_1Call};
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
