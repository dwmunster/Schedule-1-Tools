use crate::mixing::{parse_rules_file, SUBSTANCES};
use clap::Parser;
use crossbeam::queue::ArrayQueue;
use indicatif::{ProgressBar, ProgressStyle};
use mixing::{Drugs, WeedType};
use search::SearchQueueItem;
use std::cmp::min;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use topset::TopSet;

mod mixing;
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
}

// Example main function to demonstrate usage
fn main() -> Result<(), Box<dyn std::error::Error>> {
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
            effects: mixing::inherent_effects(drug),
        })
        .collect();

    let mut top = TopSet::new(args.max_results, PartialOrd::gt);

    while let Some(item) = queue.pop() {
        // We will do the first N iterations and then spawn threads to handle each of the initial substances
        let p = search::profit(&item, &rules);
        top.insert((p, item.clone()));

        let mut precompute_queue = vec![item];

        for _ in 0..min(args.num_mixins, args.precompute_layers) {
            let mut new_queue = Vec::with_capacity(precompute_queue.len() * SUBSTANCES.len());
            for item in precompute_queue {
                new_queue.extend(
                    SUBSTANCES
                        .iter()
                        .filter_map(|s| search::apply_substance(&item, *s, &rules)),
                )
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
                        item.clone(),
                        args.max_results,
                        args.num_mixins,
                    );
                    tx.send(res).unwrap();
                }
            }));
        }

        drop(tx);
        for v in rx {
            bar.inc(1);
            for subresult in v {
                if !top
                    .iter()
                    .any(|(p, i)| *p == subresult.0 && i.effects == subresult.1.effects)
                {
                    top.insert(subresult);
                    let Some(best) = top.iter().max() else {
                        continue;
                    };
                    bar.set_message(format!("{}, {:?}", best.0, best.1));
                }
            }
        }
        for handle in handles {
            handle.join().unwrap();
        }
    }

    for item in top.into_sorted_vec().iter().rev() {
        println!("{:#?}", item);
    }

    Ok(())
}
