use adapters::{path::PathAdapter, SwapMode};
use revm::{
    db::CacheDB,
    primitives::{Account, Address, HashMap, U256},
    DatabaseCommit, DatabaseRef, Evm,
};

pub mod adapters;
pub mod helpers;

pub struct Aggregator<'a, EXT, ExtDB: DatabaseRef, P: PathAdapter> {
    pub evm: Evm<'a, EXT, CacheDB<ExtDB>>,
    pub paths: Vec<P>,
}

impl<'a, EXT, ExtDB, P> Aggregator<'a, EXT, ExtDB, P>
where
    ExtDB: DatabaseRef,
    P: PathAdapter,
    <ExtDB as DatabaseRef>::Error: std::error::Error + Send + Sync + 'static,
{
    pub fn quote(&mut self, amount: U256, mode: SwapMode, splits: usize) -> Vec<U256> {
        let splitted = amount / U256::from(splits);
        let rest = amount - (splitted * U256::from(splits));
        let mut path_amounts = vec![U256::ZERO; self.paths.len()];
        for _ in 0..splits {
            let (best_path_index, best_amount, changes) = self.quote_best(splitted, mode);
            path_amounts[best_path_index] += best_amount;
            self.evm.db_mut().commit(changes);
        }
        if !rest.is_zero() {
            let (best_path_index, best_amount, changes) = self.quote_best(rest, mode);
            path_amounts[best_path_index] += best_amount;
            self.evm.db_mut().commit(changes);
        };

        path_amounts
    }

    pub fn quote_best(
        &mut self,
        amount: U256,
        mode: SwapMode,
    ) -> (usize, U256, HashMap<Address, Account>) {
        let output = self.paths[0].swap(&mut self.evm, amount, mode).unwrap();
        let mut best_index = 0;
        let mut best_amount = output.amount;
        let mut best_changes = output.state;
        for index in 1..self.paths.len() {
            let output = self.paths[index].swap(&mut self.evm, amount, mode).unwrap();
            match mode {
                SwapMode::In => {
                    if output.amount > best_amount {
                        best_amount = output.amount;
                        best_index = index;
                        best_changes = output.state;
                    }
                }
                SwapMode::Out => {
                    if output.amount < best_amount {
                        best_amount = output.amount;
                        best_index = index;
                        best_changes = output.state;
                    }
                }
            };
        }
        (best_index, best_amount, best_changes)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn it_works() {}
}
