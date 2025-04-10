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
use AggregateRouter::execute_1Call;
use ERC20::Transfer;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    ERC20,
    "../../data/abi/ERC20.json"
);

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    #[derive(Debug)]
    AggregateRouter,
    "../../data/abi/AggregateRouter.json"
);

pub static AGGREGATE_ROUTER_ADDRESS: Address = address!("5f0acdd3ec767514ff1bf7e79949640bf94576bd");

pub mod helpers;
pub mod types;
