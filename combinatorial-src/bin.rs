use bytemuck::cast_slice;
use clap::Parser;
use indicatif::ProgressIterator;
use schedule1::combinatorial::CombinatorialEncoder;
use schedule1::mixing::{parse_rules_file, Effects, SUBSTANCES};
use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(long)]
    rules: PathBuf,

    #[arg(long)]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let rules = parse_rules_file(args.rules)?;
    let mut output_file = OpenOptions::new()
        .truncate(true)
        .write(true)
        .create(true)
        .open(args.output)?;

    let encoder = CombinatorialEncoder::<34, 8>::new();

    let n_combinations = encoder.maximum_index();
    let mut data = Box::new(vec![[0u32; 16]; n_combinations as usize]);
    for idx in (0..n_combinations).progress() {
        let effects = Effects::from_bits(encoder.decode(idx)).expect("failed to decode effect");
        let row = &mut data[idx as usize];
        for (s_idx, substance) in SUBSTANCES.iter().copied().enumerate() {
            let new_effects = rules.apply(substance, effects);
            let new_idx = encoder.encode(new_effects.bits());
            row[s_idx] = new_idx;
        }
    }

    let view = cast_slice(data.as_flattened());
    output_file.write_all(view)?;

    Ok(())
}
