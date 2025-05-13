use crate::mixing::{parse_rules_file, MixtureRules, SUBSTANCES};
use crate::packing::PackedValues;
use crate::search::{base_price, profit};
use clap::Parser;
use crossbeam::queue::ArrayQueue;
use fnv::FnvBuildHasher;
use indicatif::{ProgressBar, ProgressStyle};
use mixing::Drugs;
use search::SearchQueueItem;
use std::cmp::min;
use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
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

    #[arg(long, default_value_t = 2)]
    precompute_layers: usize,

    #[arg(long, default_value_t = 0.0)]
    markup: f64,

    #[arg(long, default_value_t = false)]
    json: bool,

    #[arg(long, default_value_t = 999)]
    max_price: i64,

    #[arg(long, default_value_t = false)]
    pareto: bool,
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

    let mut queue: Vec<_> = queue
        .into_iter()
        .map(|drug| SearchQueueItem {
            drug,
            substances: PackedValues::new(),
            effects: mixing::inherent_effects(drug),
        })
        .collect();

    let net_markup = 1.0 + args.markup;

    let top = if args.pareto {
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

        top.into_sorted_vec()
    } else {
        parallel_brute_dfs(
            &rules,
            &mut queue,
            args.num_mixins,
            args.max_results,
            args.markup,
            args.max_price,
            args.precompute_layers,
        )
    };

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

fn parallel_brute_dfs(
    rules: &Arc<MixtureRules>,
    queue: &mut Vec<SearchQueueItem>,
    num_mixins: usize,
    max_results: usize,
    markup: f64,
    max_price: i64,
    precompute_layers: usize,
) -> Vec<(i64, SearchQueueItem)> {
    let mut top = TopSet::new(max_results, PartialOrd::gt);
    let net_markup = 1.0 + markup;
    while let Some(item) = queue.pop() {
        // We will do the first N iterations and then spawn threads to handle each of the initial substances
        let base = base_price(item.drug) * net_markup;
        let p = profit(base, item.substances.iter(), item.effects, rules, max_price);
        top.insert((p, item));

        let mut precompute_queue = vec![item];

        for _ in 0..min(num_mixins, precompute_layers) {
            let mut new_queue = Vec::with_capacity(precompute_queue.len() * SUBSTANCES.len());
            for item in precompute_queue {
                new_queue.extend(SUBSTANCES.iter().filter_map(|s| {
                    search::apply_substance(item.effects, *s, rules).map(|e| SearchQueueItem {
                        drug: item.drug,
                        substances: {
                            let mut vec = item.substances;
                            vec.push(*s).expect("should have room");
                            vec
                        },
                        effects: e,
                    })
                }))
            }
            precompute_queue = new_queue;
        }

        let bar = ProgressBar::new(precompute_queue.len() as u64).with_style(
            ProgressStyle::with_template(
                "[{elapsed_precise} / ETA: {eta}] {bar} {pos:>7}/{len:7}\n{wide_msg}",
            )
            .unwrap(),
        );
        bar.enable_steady_tick(Duration::from_millis(100));

        let (tx, rx) = crossbeam::channel::unbounded();
        let work_queue = Arc::new(ArrayQueue::new(precompute_queue.len()));
        for item in precompute_queue {
            work_queue.push(item).expect("should have enough room");
        }

        let mut handles = Vec::with_capacity(num_cpus::get());
        for _ in 0..num_cpus::get() {
            let work_queue = work_queue.clone();
            let tx = tx.clone();
            let rules = rules.clone();
            handles.push(thread::spawn(move || {
                while let Some(item) = work_queue.pop() {
                    let res = search::depth_first_search(
                        &rules,
                        item,
                        max_results,
                        num_mixins,
                        markup,
                        max_price,
                    );
                    tx.send(res).unwrap();
                }
            }));
        }

        drop(tx);
        for v in rx {
            bar.inc(1);
            for subresult in v {
                if top.peek().is_some_and(|(p, _)| *p >= subresult.0) {
                    // No improvements, so continue
                    continue;
                }
                let mut drain = false;
                if let Some((p, _)) = top.iter().find(|(_, i)| i.effects == subresult.1.effects) {
                    if *p >= subresult.0 {
                        // Worse version of an existing recipe, continue
                        continue;
                    }
                    // Otherwise, take out the old one.
                    drain = true;
                }
                // This, in theory, should be done in the if let block above. However, the
                // top.iter().find(...) holds onto a reference to `top`, not allowing us to drain it.
                if drain {
                    let items = top
                        .drain()
                        .filter(|(_, i)| i.effects != subresult.1.effects);
                    let mut top2 = TopSet::new(max_results, PartialOrd::gt);
                    for item in items {
                        top2.insert(item);
                    }
                    top = top2;
                }
                top.insert(subresult);
                let Some(best) = top.iter().max() else {
                    continue;
                };
                bar.set_message(format!("{}, {:?}", best.0, best.1));
            }
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }
    top.into_sorted_vec()
}
