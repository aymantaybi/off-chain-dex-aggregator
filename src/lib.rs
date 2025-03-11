use adapters::{path::PathAdapter, SwapMode};
use revm::{
    db::CacheDB,
    primitives::{Account, Address, HashMap, U256},
    DatabaseCommit, DatabaseRef, Evm,
};

pub mod adapters;

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
        let mut best_path_index = 0;
        let (mut best_amount, mut changes) =
            self.paths[0].swap(&mut self.evm, amount, mode).unwrap();
        for i in 0..self.paths.len() {
            let (a, c) = self.paths[i].swap(&mut self.evm, amount, mode).unwrap();
            match mode {
                SwapMode::In => {
                    if a > best_amount {
                        best_amount = a;
                        best_path_index = i;
                        changes = c;
                    }
                }
                SwapMode::Out => {
                    if a < best_amount {
                        best_amount = a;
                        best_path_index = i;
                        changes = c;
                    }
                }
            };
        }
        (best_path_index, best_amount, changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
