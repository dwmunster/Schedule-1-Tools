#[allow(dead_code)]
pub mod pareto;

use crate::mixing::Drugs;
use crate::mixing::{Effects, MixtureRules, Substance, SUBSTANCES};
use crate::packing::PackedValues;
use crate::search::pareto::ParetoFront;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::sync::Arc;
use topset::TopSet;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Serialize, Deserialize)]
pub struct SearchQueueItem {
    pub drug: Drugs,
    pub substances: PackedValues<Substance, 4>,
    pub effects: Effects,
}

impl SearchQueueItem {
    pub fn cost(&self) -> i64 {
        self.substances.iter().map(substance_cost).sum()
    }

    pub fn num_mixins(&self) -> usize {
        self.substances.len()
    }
}

pub fn profit<I>(
    base_price: f64,
    substances: I,
    effects: Effects,
    rules: &MixtureRules,
    max_price: i64,
) -> i64
where
    I: Iterator<Item = Substance>,
{
    let price = min(
        (base_price * rules.price_multiplier(effects)).round() as i64,
        max_price,
    );
    price - substances.map(substance_cost).sum::<i64>()
}

pub fn apply_substance(
    effects: Effects,
    substance: Substance,
    rules: &MixtureRules,
) -> Option<Effects> {
    let new_effects = rules.apply(substance, effects);
    if new_effects == effects {
        // Adding this does nothing, trim the search space by ignoring this option
        return None;
    }
    Some(new_effects)
}

pub fn depth_first_search(
    rules: &MixtureRules,
    initial: SearchQueueItem,
    max_results: usize,
    num_mixins: usize,
    markup: f64,
    max_price: i64,
) -> Vec<(i64, SearchQueueItem)> {
    let net_markup = 1.0 + markup;

    let mut stack = vec![initial];

    let mut top = TopSet::new(max_results, PartialOrd::gt);

    while let Some(item) = stack.pop() {
        let base = base_price(item.drug) * net_markup;
        let profit = profit(base, item.substances.iter(), item.effects, rules, max_price);
        let mut improvement = top
            .peek()
            .is_none_or(|(p, _): &(i64, SearchQueueItem)| *p < profit);

        let mut drain = false;
        if let Some((p, _)) = top.iter().find(|(_, i)| i.effects == item.effects) {
            if *p >= profit {
                // Worse version of an existing recipe, continue
                improvement = false;
            } else {
                // Otherwise, take out the old one.
                drain = true;
            }
        }
        // This, in theory, should be done in the if let block above. However, the
        // top.iter().find(...) holds onto a reference to `top`, not allowing us to drain it.
        if drain {
            let items = top.drain().filter(|(_, i)| i.effects != item.effects);
            let mut top2 = TopSet::new(max_results, PartialOrd::gt);
            for item in items {
                top2.insert(item);
            }
            top = top2;
        }
        if improvement {
            top.insert((profit, item));
        }

        if item.substances.len() == num_mixins {
            // If we've already assigned TOTAL_STATIONS, then we cannot add more.
            continue;
        }
        for substance in SUBSTANCES.iter().copied() {
            if let Some(eff) = apply_substance(item.effects, substance, rules) {
                let mut substances = item.substances;
                substances
                    .push(substance)
                    .expect("should have sufficient room");
                stack.push(SearchQueueItem {
                    drug: item.drug,
                    substances,
                    effects: eff,
                });
            }
        }
    }

    top.into_sorted_vec()
}

pub fn depth_first_search_pareto<F1, F2, F3>(
    rules: &MixtureRules,
    initial: SearchQueueItem,
    num_mixins: usize,
    fronts: Arc<DashMap<Effects, ParetoFront<SearchQueueItem, i64, usize, F1, F2>>>,
    new_front: F3,
) where
    F1: Fn(&SearchQueueItem) -> i64,
    F2: Fn(&SearchQueueItem) -> usize,
    F3: Fn() -> ParetoFront<SearchQueueItem, i64, usize, F1, F2>,
{
    let mut stack = vec![initial];

    while let Some(item) = stack.pop() {
        let mut f = fronts.entry(item.effects).or_insert_with(&new_front);
        if !f.value_mut().add(item) {
            // This item does not lead to a possible improvement, prune.
            continue;
        }

        if item.substances.len() == num_mixins {
            // If we've already assigned TOTAL_STATIONS, then we cannot add more.
            continue;
        }
        for substance in SUBSTANCES.iter().copied() {
            if let Some(eff) = apply_substance(item.effects, substance, rules) {
                let mut substances = item.substances;
                substances
                    .push(substance)
                    .expect("should have sufficient room");
                stack.push(SearchQueueItem {
                    drug: item.drug,
                    substances,
                    effects: eff,
                });
            }
        }
    }
}

pub fn base_price(drug: Drugs) -> f64 {
    match drug {
        Drugs::Weed(_) => 35.0,
        Drugs::Meth => 70.0,
        Drugs::Cocaine => 150.0,
    }
}

pub fn substance_cost(substance: Substance) -> i64 {
    match substance {
        Substance::Cuke => 2,
        Substance::Banana => 2,
        Substance::Paracetamol => 3,
        Substance::Donut => 3,
        Substance::Viagra => 4,
        Substance::MouthWash => 4,
        Substance::FluMedicine => 5,
        Substance::Gasoline => 5,
        Substance::EnergyDrink => 6,
        Substance::MotorOil => 6,
        Substance::MegaBean => 7,
        Substance::Chili => 7,
        Substance::Battery => 8,
        Substance::Iodine => 8,
        Substance::Addy => 9,
        Substance::HorseSemen => 9,
    }
}
