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

/// A generic item that can be part of a Pareto front.
/// It stores the original data and the computed objective values.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ParetoItem<T, K1, K2>
where
    K1: Ord + Copy,
    K2: Ord + Copy,
{
    pub data: T,
    pub objective1: K1,
    pub objective2: K2,
}
impl<T, K1, K2> ParetoItem<T, K1, K2>
where
    K1: Ord + Copy,
    K2: Ord + Copy,
{
    fn new(data: T, objective1: K1, objective2: K2) -> Self {
        Self {
            data,
            objective1,
            objective2,
        }
    }

    /// Static method to compare two sets of values and determine their domination relationship.
    #[inline]
    fn compare_raw(obj1_a: K1, obj1_b: K2, obj2_a: K1, obj2_b: K2) -> DominationResult {
        match (obj1_a.cmp(&obj2_a), obj1_b.cmp(&obj2_b)) {
            (Ordering::Less, Ordering::Less | Ordering::Equal)
            | (Ordering::Equal, Ordering::Less) => DominationResult::FirstDominates,
            (Ordering::Greater, Ordering::Equal | Ordering::Greater)
            | (Ordering::Equal, Ordering::Greater) => DominationResult::SecondDominates,
            (Ordering::Equal, Ordering::Equal) => DominationResult::Equal,
            _ => DominationResult::NonDominated,
        }
    }

    /// Compare this item with another item to determine their domination relationship.
    #[inline]
    fn compare(&self, other: &Self) -> DominationResult {
        Self::compare_raw(
            self.objective1,
            self.objective2,
            other.objective1,
            other.objective2,
        )
    }
}

/// A Pareto front that maintains a set of non-dominated items using key functions.
#[derive(Default, Debug)]
pub struct ParetoFront<T, K1, K2, F1, F2>
where
    K1: Ord + Copy,
    K2: Ord + Copy,
    F1: Fn(&T) -> K1,
    F2: Fn(&T) -> K2,
{
    items: Vec<ParetoItem<T, K1, K2>>,
    key_fn1: F1,
    key_fn2: F2,
}

impl<T, K1, K2, F1, F2> ParetoFront<T, K1, K2, F1, F2>
where
    K1: Ord + Copy,
    K2: Ord + Copy,
    F1: Fn(&T) -> K1,
    F2: Fn(&T) -> K2,
{
    /// Create a new Pareto front with the specified key functions.
    pub fn new(key_fn1: F1, key_fn2: F2) -> Self {
        Self {
            items: Vec::new(),
            key_fn1,
            key_fn2,
        }
    }

    /// Add an item to the Pareto front if it's not dominated by any existing item.
    /// Also, remove any existing items that are dominated by this new item.
    pub fn add(&mut self, data: T) -> bool {
        let objective1 = (self.key_fn1)(&data);
        let objective2 = (self.key_fn2)(&data);

        let new_item = ParetoItem::new(data, objective1, objective2);

        // Fast-path: if there are no items yet, just add the new one
        if self.items.is_empty() {
            self.items.push(new_item);
            return true;
        }

        // Track which existing items are dominated and need removal
        let mut dominated_indices = Vec::new();

        // Check if the new item is dominated by any existing item and record any existing items
        // dominated by the new item
        for (idx, item) in self.items.iter().enumerate() {
            match item.compare(&new_item) {
                DominationResult::FirstDominates | DominationResult::Equal => {
                    // New item is dominated or moot, early exit
                    return false;
                }
                DominationResult::SecondDominates => {
                    dominated_indices.push(idx);
                }
                DominationResult::NonDominated => {}
            }
        }

        // Remove dominated items in reverse order to maintain valid indices
        dominated_indices.sort_unstable();
        for idx in dominated_indices.into_iter().rev() {
            self.items.swap_remove(idx);
        }

        self.items.push(new_item);
        true
    }

    /// Get all items in the Pareto front
    pub fn get_all(&self) -> &[ParetoItem<T, K1, K2>] {
        &self.items
    }

    /// Get the number of items in the Pareto front
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the Pareto front is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Sort the Pareto front by objective 1 (primary) and then by objective 2 (secondary)
    pub fn sort(&mut self) {
        self.items
            .sort_by(|a, b| match a.objective1.cmp(&b.objective1) {
                Ordering::Equal => a.objective2.cmp(&b.objective2),
                other => other,
            });
    }

    /// Find the item with the minimum primary objective
    pub fn min_objective_1(&self) -> Option<&ParetoItem<T, K1, K2>> {
        self.items.iter().min_by_key(|item| item.objective1)
    }

    /// Find the item with the minimum secondary objective
    pub fn min_objective_2(&self) -> Option<&ParetoItem<T, K1, K2>> {
        self.items.iter().min_by_key(|item| item.objective2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pareto_item_dominates() {
        // Create items with different costs and data lengths
        let item1 = ParetoItem::new((), 10, 3);
        let item2 = ParetoItem::new((), 20, 3);
        let item3 = ParetoItem::new((), 10, 4);
        let item4 = ParetoItem::new((), 5, 2);

        // Test domination logic
        assert_eq!(item1.compare(&item2), DominationResult::FirstDominates); // Lower cost, same length
        assert_eq!(item1.compare(&item3), DominationResult::FirstDominates); // Same cost, shorter length
        assert_eq!(item4.compare(&item1), DominationResult::FirstDominates); // Lower cost, shorter length

        assert_eq!(item2.compare(&item1), DominationResult::SecondDominates); // Lower cost, same length
        assert_eq!(item3.compare(&item1), DominationResult::SecondDominates); // Same cost, shorter length
        assert_eq!(item1.compare(&item4), DominationResult::SecondDominates); // Lower cost, shorter length

        // Test non-domination
        assert_eq!(item2.compare(&item3), DominationResult::NonDominated); // Higher cost, shorter length

        // Test equal items - neither should dominate
        let item5 = ParetoItem::new((), 10, 3);
        assert_eq!(item1.compare(&item5), DominationResult::Equal);
        assert_eq!(item5.compare(&item1), DominationResult::Equal);
    }

    struct Dummy {
        cost: i64,
        data: &'static [usize],
    }

    fn k1(d: &Dummy) -> i64 {
        d.cost
    }

    fn k2(d: &Dummy) -> usize {
        d.data.len()
    }

    #[test]
    fn test_pareto_front_add() {
        let mut front = ParetoFront::new(k1, k2);

        // Adding the first item should always succeed
        assert!(front.add(Dummy {
            cost: 10,
            data: &[1, 2, 3]
        }));
        assert_eq!(front.len(), 1);

        // Adding a dominated item should fail
        assert!(!front.add(Dummy {
            cost: 20,
            data: &[1, 2, 3]
        }));
        assert_eq!(front.len(), 1);

        // Adding a non-dominated item should succeed
        assert!(front.add(Dummy {
            cost: 5,
            data: &[1, 2, 3, 4]
        }));
        assert_eq!(front.len(), 2);

        // Adding a dominating item should remove dominated items
        assert!(front.add(Dummy {
            cost: 4,
            data: &[1, 2]
        }));
        assert_eq!(front.len(), 1);

        // Check the remaining item
        let item = &front.get_all()[0];
        assert_eq!(item.data.cost, 4);
        assert_eq!(item.data.data, &[1, 2]);
    }

    #[test]
    fn test_pareto_front_sort() {
        let mut front = ParetoFront::new(k1, k2);

        // Add items in mixed order
        front.add(Dummy {
            cost: 30,
            data: &[1, 2, 3],
        });
        front.add(Dummy {
            cost: 20,
            data: &[1, 2, 3, 4],
        });
        front.add(Dummy {
            cost: 10,
            data: &[1, 2, 3, 4, 5],
        });

        // Sort the front
        front.sort();

        // Check sorted order
        let items = front.get_all();
        assert_eq!(items[0].data.cost, 10);
        assert_eq!(items[0].data.data.len(), 5);
        assert_eq!(items[1].data.cost, 20);
        assert_eq!(items[1].data.data.len(), 4);
        assert_eq!(items[2].data.cost, 30);
        assert_eq!(items[2].data.data.len(), 3);
    }

    #[test]
    fn test_pareto_front_min_methods() {
        let mut front = ParetoFront::new(k1, k2);

        // Test with empty front
        assert!(front.min_objective_1().is_none());
        assert!(front.min_objective_2().is_none());

        // Add items
        front.add(Dummy {
            cost: 30,
            data: &[1, 2, 3],
        });
        front.add(Dummy {
            cost: 20,
            data: &[1, 2, 3, 4],
        });
        front.add(Dummy {
            cost: 15,
            data: &[1, 2, 3, 4, 5],
        });
        front.add(Dummy {
            cost: 25,
            data: &[1],
        });

        // Test min cost item
        let min_cost = front.min_objective_1().unwrap();
        assert_eq!(min_cost.data.cost, 15);

        // Test min length item
        let min_length = front.min_objective_2().unwrap();
        assert_eq!(min_length.data.data.len(), 1);
    }

    #[test]
    fn test_complex_pareto_front() {
        let mut front = ParetoFront::new(k1, k2);

        // Add a series of items with different trade-offs
        front.add(Dummy {
            cost: 100,
            data: &[1],
        });
        front.add(Dummy {
            cost: 80,
            data: &[1, 2],
        });
        front.add(Dummy {
            cost: 60,
            data: &[1, 2, 3],
        });
        front.add(Dummy {
            cost: 40,
            data: &[1, 2, 3, 4],
        });
        front.add(Dummy {
            cost: 20,
            data: &[1, 2, 3, 4, 5],
        });

        // All should be on the front since none dominates another
        assert_eq!(front.len(), 5);

        // Add an item that dominates some but not all
        front.add(Dummy {
            cost: 70,
            data: &[1, 2],
        });

        // Should remove the item with cost 80 and length 2
        assert_eq!(front.len(), 5);

        // Add a strictly dominating item
        front.add(Dummy {
            cost: 10,
            data: &[],
        });

        // Should remove everything else
        assert_eq!(front.len(), 1);
        assert_eq!(front.get_all()[0].data.cost, 10);
        assert_eq!(front.get_all()[0].data.data.len(), 0);
    }
}
