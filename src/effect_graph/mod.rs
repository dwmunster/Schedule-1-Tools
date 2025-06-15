use crate::combinatorial::CombinatorialEncoder;
use crate::mixing::{Effects, MixtureRules, SUBSTANCES};
use savefile::SavefileError;
use savefile_derive::Savefile;
use std::io::Write;

const GRAPH_VERSION: u32 = 1;

#[derive(Savefile)]
pub struct EffectGraph {
    successors: Vec<[u32; SUBSTANCES.len()]>,
    predecessors: Vec<Vec<u32>>,
}

impl EffectGraph {
    pub fn new<const N: u8, const K: u8>(
        rules: &MixtureRules,
        encoder: &CombinatorialEncoder<N, K>,
    ) -> Self {
        let n_combinations = encoder.maximum_index();
        let mut successors = vec![[0u32; SUBSTANCES.len()]; n_combinations as usize];
        let mut predecessors = vec![Vec::new(); n_combinations as usize];

        for idx in 0..n_combinations {
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

        Self {
            successors,
            predecessors,
        }
    }

    pub fn serialize(&self, writer: &mut impl Write) -> Result<(), SavefileError> {
        savefile::save(writer, GRAPH_VERSION, self)
    }
}
