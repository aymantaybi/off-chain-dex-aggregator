use revm::{
    primitives::{Account, Address, HashMap, U256},
    Database, DatabaseRef, Evm,
};

use super::SwapMode;

pub trait PathAdapter {
    /// Returns the input/output amount, and the state change.
    fn swap<EXT, DB>(
        &mut self,
        evm: &mut Evm<'_, EXT, DB>,
        amount: U256,
        mode: SwapMode,
    ) -> eyre::Result<(U256, HashMap<Address, Account>)>
    where
        DB: Database + DatabaseRef,
        <DB as revm::Database>::Error: std::error::Error + Sync + Send + 'static;
}
