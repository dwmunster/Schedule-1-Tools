use crate::combinatorial::CombinatorialEncoder;
use crate::flat_storage::FlatStorage;
use crate::mixing::{Effects, MixtureRules, Substance, SUBSTANCES};
use savefile::SavefileError;
use savefile_derive::Savefile;
use std::io::Write;

type EffectIndex = u32;

pub const GRAPH_VERSION: u32 = 1;

#[derive(Savefile)]
pub struct EffectGraph<const N: u8, const K: u8> {
    successors: Vec<[EffectIndex; SUBSTANCES.len()]>,
    predecessors: FlatStorage<EffectIndex>,
    encoder: CombinatorialEncoder<N, K>,
}

impl<const N: u8, const K: u8> EffectGraph<N, K> {
    pub fn new(rules: &MixtureRules, encoder: CombinatorialEncoder<N, K>) -> Self {
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

        let predecessors = predecessors.into();

        Self {
            successors,
            predecessors,
            encoder,
        }
    }

    pub fn serialize(&self, writer: &mut impl Write) -> Result<(), SavefileError> {
        savefile::save(writer, GRAPH_VERSION, self)
    }

    pub fn num_nodes(&self) -> usize {
        self.successors.len()
    }

    pub fn encode(&self, effects: Effects) -> EffectIndex {
        self.encoder.encode(effects.bits())
    }

    pub fn decode(&self, id: EffectIndex) -> Option<Effects> {
        Effects::from_bits(self.encoder.decode(id))
    }

    pub fn successors(&self, id: EffectIndex) -> &[EffectIndex; SUBSTANCES.len()] {
        &self.successors[id as usize]
    }

    pub fn predecessors(&self, id: EffectIndex) -> &[EffectIndex] {
        self.predecessors.get(id as usize)
    }

    pub fn predecessors_with_substances(
        &self,
        id: EffectIndex,
    ) -> impl Iterator<Item = (EffectIndex, Substance)> + use<'_, N, K> {
        self.predecessors.get(id as usize).iter().map(move |n| {
            (
                *n,
                SUBSTANCES[self.successors[*n as usize]
                    .iter()
                    .position(|n2| *n2 == id)
                    .expect("failed to find matching substance")],
            )
        })
    }
}
