use criterion::{criterion_group, criterion_main, Criterion};
use schedule1::mixing::{parse_rules_file, Drugs, Effects};
use schedule1::search;
use schedule1::search::SearchQueueItem;
use std::path::PathBuf;

pub fn depth_first_search(c: &mut Criterion) {
    let rules = parse_rules_file(PathBuf::from("sch1-mix-rules.json")).expect("must parse rules");
    let initial = SearchQueueItem {
        drug: Drugs::Cocaine,
        substances: vec![],
        effects: Effects::empty(),
    };

    c.bench_function("depth_first_search", |b| {
        b.iter(|| search::depth_first_search(&rules, initial.clone(), 5, 6, 1.0))
    });
}

criterion_group!(benches, depth_first_search);
criterion_main!(benches);
