use clap::Parser;
use indicatif::ProgressIterator;
use savefile_derive::Savefile;
use schedule1::combinatorial::CombinatorialEncoder;
use schedule1::mixing::{parse_rules_file, Effects, MixtureRules, SUBSTANCES};
use std::error::Error;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

const GRAPH_VERSION: u32 = 1;

#[derive(Savefile)]
struct Graph {
    successors: Vec<[u32; 16]>,
    predecessors: Vec<Vec<u32>>,
}

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(long)]
    rules: PathBuf,

    #[arg(long)]
    graph: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    Generate,
}

fn generate_graph<const N: u8, const MAX_K: u8>(
    rules: &MixtureRules,
    encoder: &CombinatorialEncoder<N, MAX_K>,
) -> Graph {
    let n_combinations = encoder.maximum_index();
    let mut successors = vec![[0u32; 16]; n_combinations as usize];
    let mut predecessors = vec![Vec::new(); n_combinations as usize];

    for idx in (0..n_combinations).progress() {
        let effects = Effects::from_bits(encoder.decode(idx)).expect("failed to decode effect");
        let row = &mut successors[idx as usize];
        for (s_idx, substance) in SUBSTANCES.iter().copied().enumerate() {
            // Add a link to the effects after applying the substance
            let new_effects = rules.apply(substance, effects);
            let new_idx = encoder.encode(new_effects.bits());
            row[s_idx] = new_idx;

            // If we don't loop back to ourselves, add a backlink to the predecessors.
            if new_idx == idx {
                continue;
            }
            let pred = &mut predecessors[new_idx as usize];
            if !pred.contains(&idx) {
                pred.push(idx);
            }
        }
    }

    Graph {
        successors,
        predecessors,
    }
}

fn generate<const N: u8, const K: u8>(
    rules: &MixtureRules,
    encoder: &CombinatorialEncoder<N, K>,
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
    let g = generate_graph(rules, encoder);
    savefile::save(&mut file, GRAPH_VERSION, &g).map_err(|e| e.into())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let rules = parse_rules_file(args.rules)?;
    let encoder = CombinatorialEncoder::<34, 8>::new();

    match args.command {
        Command::Generate => generate(&rules, &encoder, args.graph.as_path()),
    }
}
