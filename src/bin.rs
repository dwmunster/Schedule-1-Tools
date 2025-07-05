use clap::Parser;
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use savefile_derive::Savefile;
use schedule1::combinatorial::CombinatorialEncoder;
use schedule1::effect_graph::{EffectGraph, GRAPH_VERSION};
use schedule1::flat_storage::FlatStorage;
use schedule1::mixing::{
    base_price, inherent_effects, parse_rules_file, substance_cost, Drugs, Effects, MixtureRules,
    Substance, MAX_EFFECTS, NUM_EFFECTS, SUBSTANCES,
};
use schedule1::mosp::{multiobjective_shortest_path, Cost, EffectIndex, Label, PathLength};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{stdout, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use topset::TopSet;

type FlatPaths = FlatStorage<Label>;

#[derive(Savefile, Serialize, Deserialize)]
struct FlattenedResultsFile {
    price_multipliers: Vec<u16>,
    kush: FlatPaths,
    sour_diesel: FlatPaths,
    green_crack: FlatPaths,
    granddaddy_purple: FlatPaths,
    meth_cocaine: FlatPaths,
}

const SHORTEST_PATH_VERSION: u32 = 3;

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
    },
    Lookup {
        #[arg(long)]
        routes: PathBuf,
        #[arg(long, conflicts_with = "index")]
        effects: Option<String>,
        #[arg(long)]
        index: Option<EffectIndex>,
    },
    Profit {
        #[arg(long)]
        routes: PathBuf,
        #[arg(long)]
        max_mixins: Option<PathLength>,
        #[arg(long, default_value_t = 0.)]
        markup: f64,
        #[arg(long, default_value_t = 999)]
        max_price: Cost,
        #[arg(long, default_value_t = 10)]
        max_results: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Metadata {
        #[arg(long)]
        graph: Option<PathBuf>,

        #[arg(long)]
        routes: Option<PathBuf>,
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
        .map(|s| substance_cost(s) as Cost)
        .collect::<Vec<_>>();

    multiobjective_shortest_path(graph, &costs, starting).into()
}

fn trace_path(start: Label, paths: &FlatPaths) -> Vec<Substance> {
    let mut path = Vec::with_capacity(start.length as usize);
    let mut l = start;
    while let Some((next, s)) = l.backlink() {
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

fn lookup(index: u32, labels: &FlatPaths) -> Vec<Vec<Substance>> {
    let starting_labels = labels.get(index as usize);
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

fn graph_metadata<const N: u8, const K: u8>(graph: &EffectGraph<N, K>) {
    println!("---------\nGraph metadata:");
    println!(
        "size_of::<EffectGraph<N, K>>() = {}",
        size_of::<EffectGraph<N, K>>()
    );

    let num_nodes = graph.num_nodes();
    println!("Number of nodes = {}", num_nodes);

    let backlinks: usize = (0..num_nodes)
        .map(|idx| graph.predecessors(idx as u32).len())
        .sum();
    println!("Number of backlinks = {}", backlinks);
    println!();
}

fn routes_metadata(routes: &FlattenedResultsFile) {
    println!("---------\nRoute metadata:");
    println!("size_of::<Label>() = {}", size_of::<Label>());
    let num_nodes = routes.price_multipliers.len();
    for (title, paths) in [
        ("Kush", &routes.kush),
        ("Sour Diesel", &routes.sour_diesel),
        ("Green Crack", &routes.green_crack),
        ("GDP", &routes.granddaddy_purple),
        ("Meth/Cocaine", &routes.meth_cocaine),
    ] {
        let mut total = 0usize;
        let mut counts: HashMap<usize, usize> = HashMap::new();
        let mut lengths: HashMap<PathLength, usize> = HashMap::new();

        let mut longest = TopSet::new(5, PartialOrd::gt);

        for idx in 0..num_nodes {
            let labels = paths.get(idx);
            let l = labels.len();
            total += l;
            *counts.entry(l).or_insert(0) += 1;

            if let Some(l) = labels.iter().min_by_key(|l| l.length) {
                *lengths.entry(l.length).or_insert(0) += 1;
                longest.insert((l.length, idx));
            }
        }

        let mut counts = counts.into_iter().collect::<Vec<_>>();
        counts.sort();
        println!("{title}:\n  Number of labels: {total}\n  Counts: {counts:?}");

        let mut lengths = lengths.into_iter().collect::<Vec<_>>();
        lengths.sort();
        println!("  Minimum Lengths: {lengths:?}");

        let mut longest = longest.into_sorted_vec();
        longest.reverse();
        println!("  Longest Minimum Lengths: {longest:?}");
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let rules = parse_rules_file(args.rules)?;
    let encoder = CombinatorialEncoder::<NUM_EFFECTS, MAX_EFFECTS>::new();

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
            let g: EffectGraph<NUM_EFFECTS, MAX_EFFECTS> =
                savefile::load_file(graph, GRAPH_VERSION)?;

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
                .map(|p| (p * 100.).round() as u16)
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
        Command::Search { routes, effects } => {
            let bar = ProgressBar::new_spinner();
            bar.enable_steady_tick(Duration::from_millis(100));

            bar.set_message("Loading routes");
            let shortest_paths: FlattenedResultsFile =
                savefile::load_file(routes, SHORTEST_PATH_VERSION)?;
            let target_effects =
                bitflags::parser::from_str_strict(&effects).map_err(|e| e.to_string())?;
            bar.set_message("Searching for matching routes");

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
                search_inexact(target_effects, &encoder, fp).map(|p| (*d, p, *fp))
            })
            .collect::<Vec<_>>()
            {
                bar.finish_and_clear();
                println!("{drug:?}");
                for (title, (idx, label)) in [("Lowest Cost", lowest_cost), ("Shortest", shortest)]
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

            Ok(())
        }
        Command::Lookup {
            routes,
            effects,
            index,
        } => {
            let bar = ProgressBar::new_spinner();
            bar.enable_steady_tick(Duration::from_millis(100));

            bar.set_message("Loading routes");
            let shortest_paths: FlattenedResultsFile =
                savefile::load_file(routes, SHORTEST_PATH_VERSION)?;

            let index = match (index, effects) {
                (Some(i), _) => i,
                (None, Some(e)) => {
                    let effects: Effects =
                        bitflags::parser::from_str_strict(&e).map_err(|e| e.to_string())?;
                    encoder.encode(effects.bits())
                }
                _ => panic!("index and effects cannot both be None"),
            };

            println!("Effects: {:?}", Effects::from(encoder.decode(index)));

            for (drug, paths) in [
                (Drugs::OGKush, &shortest_paths.kush),
                (Drugs::SourDiesel, &shortest_paths.sour_diesel),
                (Drugs::GreenCrack, &shortest_paths.green_crack),
                (Drugs::GranddaddyPurple, &shortest_paths.granddaddy_purple),
                (Drugs::Meth, &shortest_paths.meth_cocaine),
            ]
            .iter()
            .map(|(d, fp)| (d, lookup(index, fp)))
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
            Ok(())
        }
        Command::Profit {
            routes,
            max_mixins,
            markup,
            max_price,
            max_results,
            json,
        } => {
            let shortest_paths: FlattenedResultsFile =
                savefile::load_file(routes, SHORTEST_PATH_VERSION)?;

            let max_mixins = max_mixins.unwrap_or(PathLength::MAX);

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
                    let mult = shortest_paths.price_multipliers[idx] as f64 / 100.;
                    let best = fp
                        .get(idx)
                        .iter()
                        .filter(|label| label.length <= max_mixins)
                        .min_by_key(|l| l.cost);
                    if let Some(best) = best {
                        let sell_price = max_price.min((base_price * mult).round() as Cost) as i32;
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

                if !json {
                    println!("\n{drug}");
                }

                for (profit, sell_price, idx, label) in results {
                    let path = trace_path(*label, fp);
                    if json {
                        #[derive(Serialize)]
                        struct Output<'s> {
                            drug: Drugs,
                            effects: Effects,
                            sell_price: i32,
                            cost: Cost,
                            profit: i32,
                            ingredients: &'s [Substance],
                        }

                        serde_json::to_writer(
                            stdout(),
                            &Output {
                                drug,
                                effects: Effects::from(encoder.decode(idx as u32)),
                                sell_price,
                                cost: label.cost,
                                profit,
                                ingredients: &path,
                            },
                        )?;
                        println!();
                    } else {
                        println!(
                            "{:?}\n  Sell Price: {sell_price}\n  Cost: {}\n  Profit: {profit}\n  Ingredients: {path:?}\n",
                            Effects::from(encoder.decode(idx as u32)),
                            label.cost,
                        );
                    }
                }
            }
            Ok(())
        }
        Command::Metadata { graph, routes } => {
            if let Some(g) = graph {
                let graph: EffectGraph<NUM_EFFECTS, MAX_EFFECTS> =
                    savefile::load_file(g, GRAPH_VERSION)?;
                graph_metadata(&graph);
            }
            if let Some(r) = routes {
                let routes = savefile::load_file(r, SHORTEST_PATH_VERSION)?;
                routes_metadata(&routes);
            }
            Ok(())
        }
    }
}
