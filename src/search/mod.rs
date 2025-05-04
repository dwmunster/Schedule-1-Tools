use crate::mixing::Drugs;
use crate::mixing::{Effects, MixtureRules, Substance, SUBSTANCES};
use lockfree_object_pool::{LinearObjectPool, LinearReusable};
use std::sync::Arc;
use topset::TopSet;

#[derive(Debug, PartialOrd, PartialEq, Ord, Eq, Clone)]
pub struct SearchQueueItem {
    pub drug: Drugs,
    pub substances: Vec<Substance>,
    pub effects: Effects,
}

type InternalVec<'p> = LinearReusable<'p, Vec<Substance>>;
type InternalPool = Arc<LinearObjectPool<Vec<Substance>>>;

fn clone_with_pool<'p>(v: &InternalVec, pool: &'p InternalPool) -> InternalVec<'p> {
    let mut s = pool.pull();
    s.clone_from(v);
    s
}

struct InternalItem<'p> {
    drug: Drugs,
    substances: InternalVec<'p>,
    effects: Effects,
}

impl<'p> InternalItem<'p> {
    fn from_item(
        item: &SearchQueueItem,
        pool: &'p Arc<LinearObjectPool<Vec<Substance>>>,
    ) -> InternalItem<'p> {
        let mut substances = pool.pull();
        substances.clone_from(&item.substances);
        Self {
            drug: item.drug,
            substances,
            effects: item.effects,
        }
    }

    fn to_item(&self) -> SearchQueueItem {
        SearchQueueItem {
            drug: self.drug,
            substances: self.substances.clone(),
            effects: self.effects,
        }
    }
}

pub fn profit<'a, I>(base_price: f64, substances: I, effects: Effects, rules: &MixtureRules) -> i64
where
    I: Iterator<Item = &'a Substance>,
{
    let price = (base_price * rules.price_multiplier(effects)).round() as i64;
    price - substances.map(|s| substance_cost(*s)).sum::<i64>()
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
) -> Vec<(i64, SearchQueueItem)> {
    let net_markup = 1.0 + markup;

    let pool = Arc::new(LinearObjectPool::new(
        move || Vec::with_capacity(num_mixins),
        |v| v.clear(),
    ));

    let mut stack = vec![InternalItem::from_item(&initial, &pool)];

    let mut top = TopSet::new(max_results, PartialOrd::gt);

    while let Some(item) = stack.pop() {
        let base = base_price(item.drug) * net_markup;
        let profit = profit(base, item.substances.iter(), item.effects, rules);
        let improvement = top.peek().map(|(p, _)| profit > *p).unwrap_or(true);
        if improvement
            && !top
                .iter()
                .any(|(p, i): &(i64, SearchQueueItem)| *p == profit && i.effects == item.effects)
        {
            top.insert((profit, item.to_item()));
        }

        if item.substances.len() == num_mixins {
            // If we've already assigned TOTAL_STATIONS, then we cannot add more.
            continue;
        }
        for substance in SUBSTANCES.iter().copied() {
            if let Some(eff) = apply_substance(item.effects, substance, rules) {
                let mut substances = clone_with_pool(&item.substances, &pool);
                substances.push(substance);
                stack.push(InternalItem {
                    drug: item.drug,
                    substances,
                    effects: eff,
                });
            }
        }
    }

    top.into_sorted_vec()
}

pub fn base_price(drug: Drugs) -> f64 {
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
