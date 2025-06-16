mod mosp;

use crate::mosp::{multiobjective_shortest_path, Label};
use clap::Parser;
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use savefile_derive::Savefile;
use schedule1::combinatorial::CombinatorialEncoder;
use schedule1::effect_graph::{EffectGraph, GRAPH_VERSION};
use schedule1::mixing::{parse_rules_file, Effects, MixtureRules, Substance, SUBSTANCES};
use schedule1::search::substance_cost;
use std::collections::HashMap;
use std::error::Error;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Savefile)]
struct ShortestPaths {
    paths: Vec<Vec<Label>>,
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
        starting_effects: String,
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
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(graph_path)?;
    let g = EffectGraph::new(rules, encoder);
    g.serialize(&mut file).map_err(Into::into)
}

fn shortest_path<const N: u8, const K: u8>(
    starting: Effects,
    graph: &EffectGraph<N, K>,
) -> Result<ShortestPaths, Box<dyn Error>> {
    let costs = SUBSTANCES
        .iter()
        .copied()
        .map(|s| substance_cost(s) as u32)
        .collect::<Vec<_>>();

    Ok(ShortestPaths {
        paths: multiobjective_shortest_path(graph, &costs, starting),
    })
}

fn trace_path(start: Label, paths: &[Vec<Label>]) -> Vec<Substance> {
    let mut path = Vec::with_capacity(start.length as usize);
    let mut l = start.clone();
    while let Some((next, s)) = l.previous {
        path.push(s);
        l = *paths[next as usize]
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
    labels: ShortestPaths,
) -> Vec<Vec<Substance>> {
    let starting_labels = &labels.paths[encoder.encode(effects.bits()) as usize];
    let mut paths = Vec::with_capacity(starting_labels.len());
    for potential_path in starting_labels {
        let path = trace_path(*potential_path, &labels.paths);
        paths.push(path);
    }

    paths
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
        Command::ShortestPath {
            graph,
            starting_effects,
            output_file,
        } => {
            let mut output_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(output_file)?;
            let bar = ProgressBar::new_spinner();
            bar.enable_steady_tick(Duration::from_millis(100));
            bar.set_message("Loading graph");
            let g: EffectGraph<34, 8> = savefile::load_file(graph, GRAPH_VERSION)?;
            let starting =
                bitflags::parser::from_str_strict(&starting_effects).map_err(|e| e.to_string())?;
            bar.set_message("Finding shortest paths");
            let paths = shortest_path(starting, &g)?;
            bar.set_message("Serializing shortest paths");
            savefile::save(&mut output_file, SHORTEST_PATH_VERSION, &paths)?;
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
            let shortest_paths = savefile::load_file(routes, SHORTEST_PATH_VERSION)?;
            let target_effects =
                bitflags::parser::from_str_strict(&effects).map_err(|e| e.to_string())?;
            bar.set_message("Searching for matching routes");
            if exact {
                let paths = search_exact(target_effects, &encoder, shortest_paths);
                bar.finish_and_clear();
                for path in paths {
                    let cost: i64 = path.iter().copied().map(substance_cost).sum();
                    println!("cost: {cost}, length: {}, substances: {path:?}", path.len());
                }
            } else {
                bar.set_style(
                    ProgressStyle::with_template(
                        "{wide_bar} {human_pos} / {human_len}\n{wide_msg}",
                    )
                    .unwrap(),
                );
                bar.set_message("Searching for matching routes");
                bar.set_length(shortest_paths.paths.len() as u64);
                let mut lowest_cost = None;
                let mut shortest = None;
                for (idx, paths) in shortest_paths.paths.iter().enumerate().progress_with(bar) {
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
                if lowest_cost.is_none() || shortest.is_none() {
                    println!("No matching routes");
                    return Ok(());
                }
                let lowest_cost = lowest_cost.unwrap();
                let shortest = shortest.unwrap();

                for (title, (idx, label)) in [("Lowest Cost", lowest_cost), ("Shortest", shortest)]
                {
                    let p = trace_path(label, &shortest_paths.paths);
                    println!(
                        "{title}:\n  Effects: {:?}\n  Cost: {}\n  Length: {}\n  Path: {:?}",
                        Effects::from(encoder.decode(idx as u32)),
                        label.cost,
                        label.length,
                        p
                    )
                }
            }
            Ok(())
        }
    }
}
