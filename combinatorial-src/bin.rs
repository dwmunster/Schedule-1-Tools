use clap::Parser;
use schedule1::combinatorial::CombinatorialEncoder;
use schedule1::effect_graph::EffectGraph;
use schedule1::mixing::{parse_rules_file, MixtureRules};
use std::error::Error;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

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
    let g = EffectGraph::new(rules, encoder);
    g.serialize(&mut file).map_err(Into::into)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let rules = parse_rules_file(args.rules)?;
    let encoder = CombinatorialEncoder::<34, 8>::new();

    match args.command {
        Command::Generate => generate(&rules, &encoder, args.graph.as_path()),
    }
}
