use crate::types::PathUniswapV3Pool;
use alloy::{
    primitives::Bytes,
    sol_types::{SolCall, SolEvent},
};

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
