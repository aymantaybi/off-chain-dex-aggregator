use revm::{
    primitives::{Account, Address, EvmState, HashMap, U256},
    Database, DatabaseRef, Evm,
};

use super::{SwapMode, SwapOutput};

pub trait PathAdapter {
    /// Returns the input/output amount, and the state change.
    fn swap<EXT, DB>(
        &mut self,
        evm: &mut Evm<'_, EXT, DB>,
        amount: U256,
        mode: SwapMode,
    ) -> eyre::Result<SwapOutput>
    where
        DB: Database + DatabaseRef,
        <DB as revm::Database>::Error: std::error::Error + Sync + Send + 'static;
}
