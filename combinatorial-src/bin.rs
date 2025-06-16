mod mosp;

use crate::mosp::{multiobjective_shortest_path, Label};
use clap::Parser;
use indicatif::ProgressBar;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use savefile_derive::Savefile;
use schedule1::combinatorial::CombinatorialEncoder;
use schedule1::effect_graph::{EffectGraph, GRAPH_VERSION};
use schedule1::mixing::{parse_rules_file, Drugs, Effects, MixtureRules, Substance, SUBSTANCES};
use schedule1::search::substance_cost;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Savefile)]
struct ShortestPaths {
    paths: Vec<Vec<Label>>,
}

#[derive(Savefile, Serialize, Deserialize)]
struct ResultsFile {
    price_multipliers: Vec<f64>,
    kush: Vec<Vec<Label>>,
    sour_diesel: Vec<Vec<Label>>,
    green_crack: Vec<Vec<Label>>,
    granddaddy_purple: Vec<Vec<Label>>,
    meth_cocaine: Vec<Vec<Label>>,
}

#[derive(Savefile, Serialize, Deserialize)]
struct FlatPaths {
    paths: Vec<Label>,
    offsets: Vec<usize>,
}

impl From<Vec<Vec<Label>>> for FlatPaths {
    fn from(ragged: Vec<Vec<Label>>) -> Self {
        let num_elem = ragged.len();
        let num_paths = ragged.iter().map(|p| p.len()).sum();

        let mut paths = Vec::with_capacity(num_paths);
        let mut offsets = vec![0; num_elem + 1];

        for (idx, path) in ragged.into_iter().enumerate() {
            offsets[idx + 1] = offsets[idx] + path.len();
            paths.extend(path)
        }

        Self { paths, offsets }
    }
}

impl FlatPaths {
    pub fn get(&self, idx: usize) -> &[Label] {
        let offset = self.offsets[idx];
        let length = self.offsets[idx + 1] - offset;
        &self.paths[offset..offset + length]
    }
}

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
    Migrate {
        #[arg(long)]
        kush: PathBuf,

        #[arg(long)]
        diesel: PathBuf,

        #[arg(long)]
        green_crack: PathBuf,

        #[arg(long)]
        purple: PathBuf,

        #[arg(long)]
        meth_coke: PathBuf,

        #[arg(long)]
        output: PathBuf,
    },
    MigrateFlat {
        #[arg(long)]
        kush: PathBuf,

        #[arg(long)]
        diesel: PathBuf,

        #[arg(long)]
        green_crack: PathBuf,

        #[arg(long)]
        purple: PathBuf,

        #[arg(long)]
        meth_coke: PathBuf,

        #[arg(long)]
        output: PathBuf,
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
        Command::Migrate {
            kush,
            diesel,
            green_crack,
            purple,
            meth_coke,
            output,
        } => {
            let bar = ProgressBar::new_spinner();
            bar.enable_steady_tick(Duration::from_millis(100));

            let mut out = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&output)?;

            bar.set_message("Loading kush routes");
            let kush = savefile::load_file::<ShortestPaths, _>(kush, SHORTEST_PATH_VERSION)?.paths;

            bar.set_message("Loading diesel routes");
            let sour_diesel =
                savefile::load_file::<ShortestPaths, _>(diesel, SHORTEST_PATH_VERSION)?.paths;

            bar.set_message("Loading green crack routes");
            let green_crack =
                savefile::load_file::<ShortestPaths, _>(green_crack, SHORTEST_PATH_VERSION)?.paths;

            bar.set_message("Loading purple routes");
            let granddaddy_purple =
                savefile::load_file::<ShortestPaths, _>(purple, SHORTEST_PATH_VERSION)?.paths;

            bar.set_message("Loading meth/cocaine routes");
            let meth_cocaine =
                savefile::load_file::<ShortestPaths, _>(meth_coke, SHORTEST_PATH_VERSION)?.paths;

            bar.set_message("Computing price multipliers");
            let price_multipliers = (0..encoder.maximum_index())
                .map(|idx| rules.price_multiplier(Effects::from(encoder.decode(idx))))
                .collect::<Vec<_>>();

            let all_results = ResultsFile {
                price_multipliers,
                kush,
                sour_diesel,
                green_crack,
                granddaddy_purple,
                meth_cocaine,
            };

            bar.set_message("Serializing results");
            match output
                .extension()
                .map(|ext| ext.to_string_lossy())
                .as_deref()
            {
                Some("json") => serde_json::to_writer_pretty(&mut out, &all_results)?,
                Some("msgp") => rmp_serde::encode::write(&mut out, &all_results)?,
                _ => savefile::save(&mut out, SHORTEST_PATH_VERSION, &all_results)?,
            };
            bar.finish_and_clear();
            Ok(())
        }
        Command::MigrateFlat {
            kush,
            diesel,
            green_crack,
            purple,
            meth_coke,
            output,
        } => {
            let bar = ProgressBar::new_spinner();
            bar.enable_steady_tick(Duration::from_millis(100));

            let out = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&output)?;
            let mut writer = BufWriter::new(out);

            bar.set_message("Loading kush routes");
            let kush = savefile::load_file::<ShortestPaths, _>(kush, SHORTEST_PATH_VERSION)?
                .paths
                .into();

            bar.set_message("Loading diesel routes");
            let sour_diesel =
                savefile::load_file::<ShortestPaths, _>(diesel, SHORTEST_PATH_VERSION)?
                    .paths
                    .into();

            bar.set_message("Loading green crack routes");
            let green_crack =
                savefile::load_file::<ShortestPaths, _>(green_crack, SHORTEST_PATH_VERSION)?
                    .paths
                    .into();

            bar.set_message("Loading purple routes");
            let granddaddy_purple =
                savefile::load_file::<ShortestPaths, _>(purple, SHORTEST_PATH_VERSION)?
                    .paths
                    .into();

            bar.set_message("Loading meth/cocaine routes");
            let meth_cocaine =
                savefile::load_file::<ShortestPaths, _>(meth_coke, SHORTEST_PATH_VERSION)?
                    .paths
                    .into();

            bar.set_message("Computing price multipliers");
            let price_multipliers = (0..encoder.maximum_index())
                .map(|idx| rules.price_multiplier(Effects::from(encoder.decode(idx))))
                .collect::<Vec<_>>();

            let all_results = FlattenedResultsFile {
                price_multipliers,
                kush,
                sour_diesel,
                green_crack,
                granddaddy_purple,
                meth_cocaine,
            };

            bar.set_message("Serializing results");
            match output
                .extension()
                .map(|ext| ext.to_string_lossy())
                .as_deref()
            {
                Some("json") => serde_json::to_writer_pretty(&mut writer, &all_results)?,
                Some("msgp") => rmp_serde::encode::write(&mut writer, &all_results)?,
                _ => savefile::save(&mut writer, SHORTEST_PATH_VERSION, &all_results)?,
            };
            writer.flush()?;
            bar.finish_and_clear();
            Ok(())
        }
    }
}
