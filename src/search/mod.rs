use crate::mixing::Drugs;
use crate::mixing::{Effects, MixtureRules, Substance, SUBSTANCES};
use topset::TopSet;

#[derive(Debug, PartialOrd, PartialEq, Ord, Eq, Clone)]
pub struct SearchQueueItem {
    pub drug: Drugs,
    pub substances: Vec<Substance>,
    pub effects: Effects,
}

pub fn profit(item: &SearchQueueItem, rules: &MixtureRules) -> i64 {
    let price = (base_price(item.drug) * rules.price_multiplier(item.effects)).round() as i64;
    price
        - item
            .substances
            .iter()
            .map(|s| substance_cost(*s))
            .sum::<i64>()
}

pub fn apply_substance(
    item: &SearchQueueItem,
    substance: Substance,
    rules: &MixtureRules,
) -> Option<SearchQueueItem> {
    let mut substances = item.substances.clone();
    substances.push(substance);

    let mut eff = item.effects;
    rules.apply(substance, &mut eff);
    if item.effects == eff {
        // Adding this does nothing, trim the search space by ignoring this option
        return None;
    }
    Some(SearchQueueItem {
        drug: item.drug,
        substances,
        effects: eff,
    })
}

pub fn depth_first_search(
    rules: &MixtureRules,
    initial: SearchQueueItem,
    max_results: usize,
    num_mixins: usize,
) -> Vec<(i64, SearchQueueItem)> {
    let mut stack = vec![initial];

    let mut top = TopSet::new(max_results, PartialOrd::gt);

    while let Some(item) = stack.pop() {
        let profit = profit(&item, rules);
        let improvement = top.peek().map(|(p, _)| profit > *p).unwrap_or(true);
        if improvement
            && !top
                .iter()
                .any(|(p, i): &(i64, SearchQueueItem)| *p == profit && i.effects == item.effects)
        {
            top.insert((profit, item.clone()));
        }

        if item.substances.len() == num_mixins {
            // If we've already assigned TOTAL_STATIONS, then we cannot add more.
            continue;
        }
        for substance in SUBSTANCES.iter().copied() {
            if let Some(item) = apply_substance(&item, substance, rules) {
                stack.push(item);
            }
        }
    }

    top.into_sorted_vec()
}

fn base_price(drug: Drugs) -> f64 {
    match drug {
        Drugs::Weed(_) => 35.0,
        Drugs::Meth => 70.0,
        Drugs::Cocaine => 150.0,
    }
}

fn substance_cost(substance: Substance) -> i64 {
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
