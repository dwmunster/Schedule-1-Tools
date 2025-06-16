mod mosp;

use crate::mosp::{multiobjective_shortest_path, Label};
use clap::Parser;
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use savefile_derive::Savefile;
use schedule1::combinatorial::CombinatorialEncoder;
use schedule1::effect_graph::{EffectGraph, GRAPH_VERSION};
use schedule1::flat_storage::FlatStorage;
use schedule1::mixing::{
    inherent_effects, parse_rules_file, Drugs, Effects, MixtureRules, Substance, SUBSTANCES,
};
use schedule1::search::{base_price, substance_cost};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use topset::TopSet;

type FlatPaths = FlatStorage<Label>;

#[derive(Savefile, Serialize, Deserialize)]
struct FlattenedResultsFile {
    price_multipliers: Vec<f64>,
    kush: FlatPaths,
    sour_diesel: FlatPaths,
    green_crack: FlatPaths,
    granddaddy_purple: FlatPaths,
    meth_cocaine: FlatPaths,
}

const SHORTEST_PATH_VERSION: u32 = 1;

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(long)]
    rules: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    Generate {
        #[arg(long)]
        graph: PathBuf,
    },
    ShortestPath {
        #[arg(long)]
        graph: PathBuf,
        #[arg(long)]
        output_file: PathBuf,
    },
    Search {
        #[arg(long)]
        routes: PathBuf,
        #[arg(long)]
        effects: String,
        #[arg(long, default_value_t = false)]
        exact: bool,
    },
    Profit {
        #[arg(long)]
        routes: PathBuf,
        #[arg(long)]
        max_mixins: Option<u32>,
        #[arg(long, default_value_t = 0.)]
        markup: f64,
        #[arg(long, default_value_t = 999)]
        max_price: u32,
        #[arg(long, default_value_t = 10)]
        max_results: usize,
    },
}

fn generate<const N: u8, const K: u8>(
    rules: &MixtureRules,
    encoder: CombinatorialEncoder<N, K>,
    graph_path: &Path,
) -> Result<(), Box<dyn Error>> {
    if graph_path.is_file() {
        println!("'{graph_path:?}' exists, refusing to overwrite");
        return Ok(());
    }
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(graph_path)?;
    let mut writer = BufWriter::new(file);
    let g = EffectGraph::new(rules, encoder);
    g.serialize(&mut writer)?;
    writer.flush().map_err(Into::into)
}

fn shortest_path<const N: u8, const K: u8>(
    starting: Effects,
    graph: &EffectGraph<N, K>,
) -> FlatPaths {
    let costs = SUBSTANCES
        .iter()
        .copied()
        .map(|s| substance_cost(s) as u32)
        .collect::<Vec<_>>();

    multiobjective_shortest_path(graph, &costs, starting).into()
}

fn trace_path(start: Label, paths: &FlatPaths) -> Vec<Substance> {
    let mut path = Vec::with_capacity(start.length as usize);
    let mut l = start;
    while let Some((next, s)) = l.previous {
        path.push(s);
        l = *paths
            .get(next as usize)
            .iter()
            .find(|candidate| candidate.length == l.length - 1)
            .expect("should find connected path");
    }
    // Since we started at the target and worked back to the root node, flip the order.
    path.reverse();
    path
}

fn search_exact<const N: u8, const K: u8>(
    effects: Effects,
    encoder: &CombinatorialEncoder<N, K>,
    labels: &FlatPaths,
) -> Vec<Vec<Substance>> {
    let starting_labels = labels.get(encoder.encode(effects.bits()) as usize);
    let mut paths = Vec::with_capacity(starting_labels.len());
    for potential_path in starting_labels {
        let path = trace_path(*potential_path, labels);
        paths.push(path);
    }

    paths
}

fn search_inexact<const N: u8, const K: u8>(
    target_effects: Effects,
    encoder: &CombinatorialEncoder<N, K>,
    labels: &FlatPaths,
) -> Option<((usize, Label), (usize, Label))> {
    let mut lowest_cost = None;
    let mut shortest = None;
    for idx in 0..encoder.maximum_index() as usize {
        let paths = labels.get(idx);
        // only consider reachable effects
        if paths.is_empty() {
            continue;
        }
        let current_effects = Effects::from(encoder.decode(idx as u32));
        if !current_effects.contains(target_effects) {
            continue;
        }
        for path in paths {
            if path.cost < lowest_cost.get_or_insert((idx, *path)).1.cost {
                lowest_cost = Some((idx, *path));
            }
            if path.length < shortest.get_or_insert((idx, *path)).1.length {
                shortest = Some((idx, *path));
            }
        }
    }
    lowest_cost.map(|(idx, path)| ((idx, path), shortest.unwrap()))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let rules = parse_rules_file(args.rules)?;
    let encoder = CombinatorialEncoder::<34, 8>::new();

    match args.command {
        Command::Generate { graph } => {
            let bar = ProgressBar::new_spinner();
            bar.set_message("Building graph");
            bar.enable_steady_tick(Duration::from_millis(100));
            generate(&rules, encoder, graph.as_path())?;
            bar.finish_and_clear();
            Ok(())
        }
        Command::ShortestPath { graph, output_file } => {
            let output_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(output_file)?;
            let mut writer = BufWriter::new(output_file);
            let bar = ProgressBar::new_spinner();
            bar.enable_steady_tick(Duration::from_millis(100));
            bar.set_message("Loading graph");
            let g: EffectGraph<34, 8> = savefile::load_file(graph, GRAPH_VERSION)?;

            bar.set_style(
                ProgressStyle::with_template("{wide_bar} {pos}/{len}\n{wide_msg}").unwrap(),
            );
            bar.set_message("Finding shortest paths");
            bar.set_length(5);
            let mut paths = [
                Drugs::OGKush,
                Drugs::SourDiesel,
                Drugs::GreenCrack,
                Drugs::GranddaddyPurple,
                Drugs::Meth,
            ]
            .iter()
            .progress_with(bar.clone())
            .copied()
            .map(|d| shortest_path(inherent_effects(d), &g))
            .collect::<Vec<_>>();

            let meth_cocaine = paths.pop().expect("should not be empty");
            let granddaddy_purple = paths.pop().expect("should not be empty");
            let green_crack = paths.pop().expect("should not be empty");
            let sour_diesel = paths.pop().expect("should not be empty");
            let kush = paths.pop().expect("should not be empty");

            bar.set_style(ProgressStyle::default_spinner());
            bar.set_message("Computing price multipliers");
            let price_multipliers = (0..encoder.maximum_index())
                .map(|idx| rules.price_multiplier(Effects::from(encoder.decode(idx))))
                .collect::<Vec<_>>();

            let paths = FlattenedResultsFile {
                price_multipliers,
                kush,
                sour_diesel,
                green_crack,
                granddaddy_purple,
                meth_cocaine,
            };

            bar.set_message("Serializing shortest paths");
            savefile::save(&mut writer, SHORTEST_PATH_VERSION, &paths)?;
            writer.flush()?;
            bar.finish_and_clear();
            Ok(())
        }
        Command::Search {
            routes,
            effects,
            exact,
        } => {
            let bar = ProgressBar::new_spinner();
            bar.enable_steady_tick(Duration::from_millis(100));

            bar.set_message("Loading routes");
            let shortest_paths: FlattenedResultsFile =
                savefile::load_file(routes, SHORTEST_PATH_VERSION)?;
            let target_effects =
                bitflags::parser::from_str_strict(&effects).map_err(|e| e.to_string())?;
            bar.set_message("Searching for matching routes");
            if exact {
                for (drug, paths) in [
                    (Drugs::OGKush, &shortest_paths.kush),
                    (Drugs::SourDiesel, &shortest_paths.sour_diesel),
                    (Drugs::GreenCrack, &shortest_paths.green_crack),
                    (Drugs::GranddaddyPurple, &shortest_paths.granddaddy_purple),
                    (Drugs::Meth, &shortest_paths.meth_cocaine),
                ]
                .iter()
                .map(|(d, fp)| (d, search_exact(target_effects, &encoder, fp)))
                .collect::<Vec<_>>()
                {
                    println!("{drug:?}");
                    for path in paths {
                        let cost: i64 = path.iter().copied().map(substance_cost).sum();
                        println!(
                            "  cost: {cost}, length: {}, substances: {path:?}",
                            path.len()
                        );
                    }
                    println!();
                }

                bar.finish_and_clear();
            } else {
                bar.set_message("Searching for matching routes");
                for (drug, (lowest_cost, shortest), paths) in [
                    (Drugs::OGKush, &shortest_paths.kush),
                    (Drugs::SourDiesel, &shortest_paths.sour_diesel),
                    (Drugs::GreenCrack, &shortest_paths.green_crack),
                    (Drugs::GranddaddyPurple, &shortest_paths.granddaddy_purple),
                    (Drugs::Meth, &shortest_paths.meth_cocaine),
                ]
                .par_iter()
                .filter_map(|(d, fp)| {
                    search_inexact(target_effects, &encoder, &fp).map(|p| (*d, p, *fp))
                })
                .collect::<Vec<_>>()
                {
                    bar.finish_and_clear();
                    println!("{drug:?}");
                    for (title, (idx, label)) in
                        [("Lowest Cost", lowest_cost), ("Shortest", shortest)]
                    {
                        let p = trace_path(label, paths);
                        println!(
                            "  {title}:\n    Effects: {:?}\n    Cost: {}\n    Length: {}\n    Path: {:?}",
                            Effects::from(encoder.decode(idx as u32)),
                            label.cost,
                            label.length,
                            p
                        )
                    }
                    println!();
                }
            }
            Ok(())
        }
        Command::Profit {
            routes,
            max_mixins,
            markup,
            max_price,
            max_results,
        } => {
            let shortest_paths: FlattenedResultsFile =
                savefile::load_file(routes, SHORTEST_PATH_VERSION)?;

            let max_mixins = max_mixins.unwrap_or(999);

            for (drug, fp, results) in [
                (Drugs::OGKush, &shortest_paths.kush),
                (Drugs::SourDiesel, &shortest_paths.sour_diesel),
                (Drugs::GreenCrack, &shortest_paths.green_crack),
                (Drugs::GranddaddyPurple, &shortest_paths.granddaddy_purple),
                (Drugs::Meth, &shortest_paths.meth_cocaine),
                (Drugs::Cocaine, &shortest_paths.meth_cocaine),
            ]
            .par_iter()
            .copied()
            .map(|(d, fp)| {
                let mut top = TopSet::new(max_results, PartialOrd::gt);
                let base_price = base_price(d) * (1. + markup);
                for idx in 0..encoder.maximum_index() as usize {
                    let mult = shortest_paths.price_multipliers[idx];
                    let best = fp
                        .get(idx)
                        .iter()
                        .filter(|label| label.length <= max_mixins)
                        .min_by_key(|l| l.cost);
                    if let Some(best) = best {
                        let sell_price = max_price.min((base_price * mult).round() as u32) as i32;
                        let profit = sell_price - best.cost as i32;
                        top.insert((profit, sell_price, idx, best));
                    }
                }

                (d, fp, top)
            })
            .collect::<Vec<_>>()
            {
                let mut results = results.into_sorted_vec();
                results.reverse();
                println!("{drug}");
                for (profit, sell_price, idx, label) in results {
                    let path = trace_path(*label, fp);
                    println!(
                        "{:?}\n  Sell Price: {sell_price}\n  Cost: {}\n  Profit: {profit}\n  Ingredients: {path:?}\n",
                        Effects::from(encoder.decode(idx as u32)),
                        label.cost,
                    );
                }
                println!();
            }
            Ok(())
        }
    }
}
