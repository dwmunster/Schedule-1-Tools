//! Implements the Multiobjective Shortest Path algorithm described in Maristany de las Casas,
//! Sedeño-Noda, Borndörfer. An Improved Multiobjective Shortest Path Algorithm. Computers and
//! Operations Research 135 (2021).

use crate::effect_graph::EffectGraph;
use crate::mixing::{Effects, Substance, SUBSTANCES};
use priority_queue::PriorityQueue;
use savefile_derive::Savefile;
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, Reverse};

pub type EffectIndex = u32;
pub type Cost = u16;
pub type PathLength = u8;

const NICHE: EffectIndex = EffectIndex::MAX;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Savefile, Serialize, Deserialize)]
pub struct Label {
    pub length: PathLength,
    pub cost: Cost,
    previous_substance: Substance,
    backlink: EffectIndex,
}

impl Label {
    pub fn backlink(&self) -> Option<(EffectIndex, Substance)> {
        match self.backlink {
            NICHE => None,
            _ => Some((self.backlink, self.previous_substance)),
        }
    }
}

type Queue = PriorityQueue<EffectIndex, Reverse<Label>>;

fn label_nondominated_nonequal(label: Label, existing: &[Label]) -> bool {
    for ex in existing {
        match (ex.length.cmp(&label.length), ex.cost.cmp(&label.cost)) {
            // If an existing label dominates the candidate, bail early
            (Ordering::Less, Ordering::Less | Ordering::Equal)
            | (Ordering::Equal, Ordering::Less) => return false,
            // This should never happen, log it but return true
            (Ordering::Greater, Ordering::Equal | Ordering::Greater)
            | (Ordering::Equal, Ordering::Greater) => {
                eprintln!("new label dominates existing labels! {ex:?} < {label:?}");
                return true;
            }
            // If equivalent to an existing label, we do not want to add it since we only track a
            // minimal set of efficient paths
            (Ordering::Equal, Ordering::Equal) => return false,
            // If non-dominated, continue searching
            _ => {}
        }
    }
    // If we checked all existing labels, this new one must be non-dominated and non-equivalent.
    true
}

fn next_candidate_label(
    node: EffectIndex,
    predecessors: impl Iterator<Item = (EffectIndex, Substance)>,
    substance_costs: &[Cost],
    permanent_labels: &[Vec<Label>],
) -> Option<Label> {
    let mut new_candidate = None;
    let existing_labels = &permanent_labels[node as usize];
    for (pred, sub) in predecessors {
        for old_label in &permanent_labels[pred as usize] {
            let new_label = Label {
                length: old_label.length + 1,
                cost: old_label.cost + substance_costs[sub as usize],
                previous_substance: sub,
                backlink: node,
            };
            // Test for dominance of existing items over this new candidate
            if label_nondominated_nonequal(new_label, existing_labels) {
                if new_label < *new_candidate.get_or_insert(new_label) {
                    new_candidate = Some(new_label);
                }
                break;
            }
        }
    }
    new_candidate
}

fn propagate(
    new_label: Label,
    child: EffectIndex,
    child_permanent_labels: &[Label],
    pending: &mut Queue,
) {
    if !label_nondominated_nonequal(new_label, child_permanent_labels) {
        return;
    }

    pending.push_increase(child, Reverse(new_label));
}

pub fn multiobjective_shortest_path<const N: u8, const K: u8>(
    graph: &EffectGraph<N, K>,
    substance_costs: &[Cost],
    starting_node: Effects,
) -> Vec<Vec<Label>> {
    let mut permanent_labels = vec![Vec::new(); graph.num_nodes()];
    let mut pending = Queue::new();
    pending.push(
        graph.encode(starting_node),
        Reverse(Label {
            length: 0,
            cost: 0,
            previous_substance: Substance::Cuke,
            backlink: NICHE,
        }),
    );

    while let Some((node, label)) = pending.pop() {
        permanent_labels[node as usize].push(label.0);
        if let Some(candidate) = next_candidate_label(
            node,
            graph.predecessors_with_substances(node),
            substance_costs,
            &permanent_labels,
        ) {
            pending.push(node, Reverse(candidate));
        }
        for (idx, child) in graph.successors(node).iter().enumerate() {
            propagate(
                Label {
                    length: label.0.length + 1,
                    cost: label.0.cost + substance_costs[idx],
                    previous_substance: SUBSTANCES[idx],
                    backlink: node,
                },
                *child,
                &permanent_labels[*child as usize],
                &mut pending,
            );
        }
    }

    permanent_labels
}
