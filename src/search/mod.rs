#[allow(dead_code)]
pub mod pareto;

use crate::mixing::Drugs;
use crate::mixing::{Effects, MixtureRules, Substance, SUBSTANCES};
use crate::packing::PackedValues;
use crate::search::pareto::ParetoFront;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::min;
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::ops::{Deref, DerefMut};

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

// #[derive(Debug)]
pub struct ParetoSearchFront(
    ParetoFront<
        SearchQueueItem,
        i64,
        usize,
        fn(&SearchQueueItem) -> i64,
        fn(&SearchQueueItem) -> usize,
    >,
);

impl ParetoSearchFront {
    pub fn new() -> Self {
        ParetoSearchFront(ParetoFront::new(
            SearchQueueItem::cost,
            SearchQueueItem::num_mixins,
        ))
    }
}

impl Default for ParetoSearchFront {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for ParetoSearchFront {
    type Target = ParetoFront<
        SearchQueueItem,
        i64,
        usize,
        fn(&SearchQueueItem) -> i64,
        fn(&SearchQueueItem) -> usize,
    >;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ParetoSearchFront {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Serialize for ParetoSearchFront {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.items.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ParetoSearchFront {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut p = Self::default();
        p.0.items = Deserialize::deserialize(deserializer)?;
        Ok(p)
    }
}

pub fn depth_first_search_pareto<S>(
    rules: &MixtureRules,
    initial: SearchQueueItem,
    num_mixins: usize,
    fronts: &mut HashMap<Effects, ParetoSearchFront, S>,
) where
    S: BuildHasher,
{
    let mut stack = vec![initial];

    while let Some(item) = stack.pop() {
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
                let item = SearchQueueItem {
                    drug: item.drug,
                    substances,
                    effects: eff,
                };
                let f = fronts.entry(item.effects).or_default();
                if !f.0.add(item) {
                    // This item does not lead to a possible improvement, prune.
                    continue;
                }

                stack.push(item);
            }
        }
    }
}

pub fn base_price(drug: Drugs) -> f64 {
    match drug {
        Drugs::OGKush | Drugs::SourDiesel | Drugs::GreenCrack | Drugs::GranddaddyPurple => 35.0,
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
