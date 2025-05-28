use crate::mixing::parse_rules_file;
use crate::packing::PackedValues;
use crate::search::{base_price, profit};
use clap::Parser;
use fnv::FnvBuildHasher;
use mixing::Drugs;
use search::SearchQueueItem;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use topset::TopSet;

mod mixing;
#[allow(dead_code)]
mod packing;
mod search;

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

    #[arg(long, default_value_t = 0.0)]
    markup: f64,

    #[arg(long, default_value_t = false)]
    json: bool,

    #[arg(long, default_value_t = 999)]
    max_price: i64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let rules = Arc::new(parse_rules_file(args.rules)?);

    let mut queue: Vec<_> = vec![];

    if args.cocaine {
        queue.push(Drugs::Cocaine);
    }
    if args.meth {
        queue.push(Drugs::Meth);
    }
    if args.kush || args.all_weed {
        queue.push(Drugs::OGKush);
    }
    if args.diesel || args.all_weed {
        queue.push(Drugs::SourDiesel);
    }
    if args.crack || args.all_weed {
        queue.push(Drugs::GreenCrack);
    }
    if args.purple || args.all_weed {
        queue.push(Drugs::GranddaddyPurple);
    }

    let queue: Vec<_> = queue
        .into_iter()
        .map(|drug| SearchQueueItem {
            drug,
            substances: PackedValues::new(),
            effects: mixing::inherent_effects(drug),
        })
        .collect();

    let net_markup = 1.0 + args.markup;

    let mut fronts = HashMap::with_capacity_and_hasher(10_000_000, FnvBuildHasher::default());

    let mut top = TopSet::new(args.max_results, PartialOrd::gt);

    for item in queue {
        search::depth_first_search_pareto(&rules, item, args.num_mixins, &mut fronts);
    }

    for (effects, f) in fronts {
        if let Some(min) = f
            .items
            .iter()
            .filter(|i| i.objective2 <= args.num_mixins)
            .min_by_key(|i| i.objective1)
        {
            top.insert((
                profit(
                    base_price(min.data.drug) * net_markup,
                    min.data.substances.iter(),
                    effects,
                    &rules,
                    999,
                ),
                min.data,
            ));
        }
    }

    let top = top.into_sorted_vec();

    for item in top.iter().rev() {
        let val = (
            &item.0,
            f64::min(
                rules.price_multiplier(item.1.effects) * base_price(item.1.drug) * net_markup,
                args.max_price as f64,
            )
            .round() as i64,
            &item.1,
        );
        if args.json {
            println!("{},", serde_json::to_string(&val).unwrap());
        } else {
            println!("{:#?}", &val);
        }
    }

    Ok(())
}
