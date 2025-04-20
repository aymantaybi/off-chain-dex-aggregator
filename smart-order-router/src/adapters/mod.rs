use revm::primitives::{EvmState, U256};

pub mod path;

#[derive(Debug, Clone, Copy)]
pub enum SwapMode {
    In,
    Out,
}

#[derive(Debug, Clone)]
pub struct SwapOutput {
    pub amount: U256,
    pub state: EvmState,
}

impl SwapOutput {
    pub fn new(amount: U256) -> Self {
        let state = EvmState::default();
        Self { amount, state }
    }
}
