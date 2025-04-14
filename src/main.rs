use crate::mixing::{parse_rules_file, Effect, Substance, SUBSTANCES};
use clap::Parser;
use std::collections::BTreeSet;
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
}

fn base_price(drug: Drugs) -> f64 {
    match drug {
        Drugs::Weed(_) => 35.0,
        Drugs::Meth => 70.0,
    }
}

fn inherent_effects(drug: Drugs) -> BTreeSet<Effect> {
    match drug {
        Drugs::Weed(WeedType::OGKush) => [Effect::Calming].into(),
        Drugs::Weed(WeedType::SourDiesel) => [Effect::Refreshing].into(),
        Drugs::Weed(WeedType::GreenCrack) => [Effect::Energizing].into(),
        Drugs::Weed(WeedType::GranddaddyPurple) => [Effect::Sedating].into(),
        _ => BTreeSet::new(),
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

    #[arg(long)]
    num_mixins: usize,
}

// Example main function to demonstrate usage
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let rules = parse_rules_file(args.rules)?;

    let mut queue: Vec<_> = [
        Drugs::Weed(WeedType::OGKush),
        Drugs::Weed(WeedType::SourDiesel),
        Drugs::Weed(WeedType::GreenCrack),
        Drugs::Weed(WeedType::GranddaddyPurple),
    ]
    .into_iter()
    .map(|drug| (drug, vec![], inherent_effects(drug)))
    .collect();

    let mut top = TopSet::new(5, PartialOrd::gt);

    while let Some((item, substances, effects)) = queue.pop() {
        let price = (base_price(item) * rules.price_multiplier(effects.iter())).round() as i64;
        let profit = price - substances.iter().map(|s| substance_cost(*s)).sum::<i64>();
        if !top.iter().any(|(_, _, _, e)| e == &effects) {
            top.insert((profit, item, substances.clone(), effects.clone()));
        }

        if substances.len() == args.num_mixins {
            // If we've already assigned TOTAL_STATIONS, then we cannot add more.
            continue;
        }
        for substance in SUBSTANCES.iter().copied() {
            let mut substances = substances.clone();
            substances.push(substance);

            let mut eff = effects.clone();
            rules.apply(substance, &mut eff);
            if effects == eff {
                // Adding this does nothing, trim the search space by ignoring this option
                continue;
            }
            queue.push((item, substances, eff));
        }
    }

    for item in top.into_sorted_vec().iter().rev() {
        println!("{:?}", item);
    }

    Ok(())
}
