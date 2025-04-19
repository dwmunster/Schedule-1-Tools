use crate::mixing::{parse_rules_file, Effects, MixtureRules, Substance, SUBSTANCES};
use clap::Parser;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::path::PathBuf;
use topset::TopSet;

mod mixing;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
enum WeedType {
    OGKush,
    SourDiesel,
    GreenCrack,
    GranddaddyPurple,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
enum Drugs {
    Weed(WeedType),
    Meth,
    Cocaine,
}

fn base_price(drug: Drugs) -> f64 {
    match drug {
        Drugs::Weed(_) => 35.0,
        Drugs::Meth => 70.0,
        Drugs::Cocaine => 150.0,
    }
}

fn inherent_effects(drug: Drugs) -> Effects {
    match drug {
        Drugs::Weed(WeedType::OGKush) => Effects::Calming,
        Drugs::Weed(WeedType::SourDiesel) => Effects::Refreshing,
        Drugs::Weed(WeedType::GreenCrack) => Effects::Energizing,
        Drugs::Weed(WeedType::GranddaddyPurple) => Effects::Sedating,
        _ => Effects::empty(),
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

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(long)]
    rules: PathBuf,

    #[arg(long, default_value_t = false)]
    cocaine: bool,

    #[arg(long, default_value_t = false)]
    meth: bool,

    #[arg(long, default_value_t = false, conflicts_with = "all_weed")]
    kush: bool,

    #[arg(long, default_value_t = false, conflicts_with = "all_weed")]
    diesel: bool,

    #[arg(long, default_value_t = false, conflicts_with = "all_weed")]
    crack: bool,

    #[arg(long, default_value_t = false, conflicts_with = "all_weed")]
    purple: bool,

    #[arg(long, default_value_t = false)]
    all_weed: bool,

    #[arg(long)]
    num_mixins: usize,

    #[arg(long)]
    max_results: usize,
}

// Example main function to demonstrate usage
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let rules = parse_rules_file(args.rules)?;

    let mut queue: Vec<_> = vec![];

    if args.cocaine {
        queue.push(Drugs::Cocaine);
    }
    if args.meth {
        queue.push(Drugs::Meth);
    }
    if args.kush || args.all_weed {
        queue.push(Drugs::Weed(WeedType::OGKush));
    }
    if args.diesel || args.all_weed {
        queue.push(Drugs::Weed(WeedType::SourDiesel));
    }
    if args.crack || args.all_weed {
        queue.push(Drugs::Weed(WeedType::GreenCrack));
    }
    if args.purple || args.all_weed {
        queue.push(Drugs::Weed(WeedType::GranddaddyPurple));
    }

    let mut queue: Vec<_> = queue
        .into_iter()
        .map(|drug| SearchQueueItem {
            drug,
            substances: Vec::new(),
            effects: inherent_effects(drug),
        })
        .collect();

    let mut top = TopSet::new(args.max_results, PartialOrd::gt);

    while let Some(item) = queue.pop() {
        // We will do the first iteration and then spawn threads to handle each of the initial substances
        let p = profit(&item, &rules);
        top.insert((p, item.clone()));

        let level_one: Vec<_> = SUBSTANCES
            .iter()
            .filter_map(|s| apply_substance(&item, *s, &rules))
            .collect();

        for subresult in level_one
            .into_par_iter()
            .flat_map(|i| depth_first_search(&rules, i, args.max_results, args.num_mixins))
            .collect::<Vec<_>>()
        {
            if !top
                .iter()
                .any(|(p, i)| *p == subresult.0 && i.effects == subresult.1.effects)
            {
                top.insert(subresult);
            }
        }
    }

    for item in top.into_sorted_vec().iter().rev() {
        println!("{:?}", item);
    }

    Ok(())
}

#[derive(Debug, PartialOrd, PartialEq, Ord, Eq, Clone)]
struct SearchQueueItem {
    drug: Drugs,
    substances: Vec<Substance>,
    effects: Effects,
}

fn profit(item: &SearchQueueItem, rules: &MixtureRules) -> i64 {
    let price = (base_price(item.drug) * rules.price_multiplier(item.effects)).round() as i64;
    price
        - item
            .substances
            .iter()
            .map(|s| substance_cost(*s))
            .sum::<i64>()
}

fn apply_substance(
    item: &SearchQueueItem,
    substance: Substance,
    rules: &MixtureRules,
) -> Option<SearchQueueItem> {
    let mut substances = item.substances.clone();
    substances.push(substance);

    let mut eff = item.effects.clone();
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

fn depth_first_search(
    rules: &MixtureRules,
    initial: SearchQueueItem,
    max_results: usize,
    num_mixins: usize,
) -> Vec<(i64, SearchQueueItem)> {
    let mut stack: Vec<_> = vec![initial];

    let mut top = TopSet::new(max_results, PartialOrd::gt);

    while let Some(item) = stack.pop() {
        let profit = profit(&item, rules);
        if !top
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
