use crate::search::SearchQueueItem;
use duckdb::{params, Connection};
use std::cmp::Ordering;

/// Represents the possible domination relationships between two items.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum DominationResult {
    /// The first item dominates the second item.
    FirstDominates,
    /// The second item dominates the first item.
    SecondDominates,
    /// Neither item dominates the other.
    NonDominated,
    /// The two items have identical objectives.
    Equal,
}

#[inline]
fn compare<K1: Ord, K2: Ord>(obj1_a: K1, obj1_b: K2, obj2_a: K1, obj2_b: K2) -> DominationResult {
    match (obj1_a.cmp(&obj2_a), obj1_b.cmp(&obj2_b)) {
        (Ordering::Less, Ordering::Less | Ordering::Equal) | (Ordering::Equal, Ordering::Less) => {
            DominationResult::FirstDominates
        }
        (Ordering::Greater, Ordering::Equal | Ordering::Greater)
        | (Ordering::Equal, Ordering::Greater) => DominationResult::SecondDominates,
        (Ordering::Equal, Ordering::Equal) => DominationResult::Equal,
        _ => DominationResult::NonDominated,
    }
}

pub struct ParetoFrontDB<'conn> {
    connection: &'conn Connection,
}

impl<'conn> ParetoFrontDB<'conn> {
    pub fn try_new(connection: &'conn Connection) -> Result<Self, duckdb::Error> {
        connection.execute_batch(
            r#"CREATE SEQUENCE IF NOT EXISTS pareto_id;
                CREATE TABLE IF NOT EXISTS pareto_front (
                    id INTEGER PRIMARY KEY DEFAULT NEXTVAL('pareto_id'),
                    drug UTINYINT NOT NULL,
                    effects UBIGINT NOT NULL,
                    substances HUGEINT NOT NULL,
                    cost USMALLINT NOT NULL,
                    num_mixins UTINYINT NOT NULL,
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_pareto_id ON pareto_front(id);
                CREATE INDEX IF NOT EXISTS idx_drug_effect ON pareto_front(drug, effects);"#,
        )?;
        Ok(Self { connection })
    }

    pub fn add(&self, item: SearchQueueItem) -> Result<bool, duckdb::Error> {
        let mut stmt = self.connection.prepare_cached(
            r#"SELECT id, cost, num_mixins FROM pareto_front
        WHERE drug=?
        AND effects=?"#,
        )?;
        let rows = stmt.query_map(params![item.drug as u8, item.effects.bits()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        let matches = rows.collect::<Result<Vec<(i32, u16, u8)>, _>>()?;

        let item_cost = item.cost();
        let num_mixins = item.num_mixins();

        let mut dominated = Vec::new();
        for (id, cost, mixins) in matches {
            match compare(item_cost, num_mixins, cost as i64, mixins as usize) {
                DominationResult::FirstDominates => {
                    // If the new item dominates an old one, we want to remove those old ones.
                    dominated.push(id);
                }
                DominationResult::SecondDominates | DominationResult::Equal => {
                    // If the new item is dominated or is moot, exit early
                    return Ok(false);
                }
                DominationResult::NonDominated => {}
            }
        }

        let mut add_stmt = self
            .connection
            .prepare_cached(r#"INSERT INTO pareto_front VALUES (DEFAULT, ?, ?, ?, ?, ?)"#)?;

        add_stmt.insert(params![
            item.drug as u8,
            item.effects.bits(),
            item.substances.bits() as i128,
            item_cost as u16,
            num_mixins as u8
        ])?;

        if !dominated.is_empty() {
            let mut delete_stmt = self
                .connection
                .prepare_cached(r#"DELETE FROM pareto_front WHERE id = ?"#)?;
            for id in dominated {
                delete_stmt.execute([id])?;
            }
        }

        Ok(true)
    }
}
