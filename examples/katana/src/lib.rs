use alloy::{
    sol,
    sol_types::{SolCall, SolEvent},
};
use revm::primitives::{
        address, Address,
    };

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
