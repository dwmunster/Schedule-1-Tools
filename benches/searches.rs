use criterion::{criterion_group, criterion_main, Criterion};
use fnv::FnvHashMap;
use schedule1::mixing::{parse_rules_file, Drugs, Effects};
use schedule1::packing::PackedValues;
use schedule1::search;
use schedule1::search::{base_price, profit, SearchQueueItem};
use std::path::PathBuf;
use topset::TopSet;

pub fn depth_first_search(c: &mut Criterion) {
    let rules = parse_rules_file(PathBuf::from("sch1-mix-rules.json")).expect("must parse rules");
    let initial = SearchQueueItem {
        drug: Drugs::Cocaine,
        substances: PackedValues::new(),
        effects: Effects::empty(),
    };

    c.bench_function("depth_first_search", |b| {
        b.iter(|| search::depth_first_search(&rules, initial, 5, 6, 1.0, 999))
    });
}

pub fn pareto(c: &mut Criterion) {
    let rules = parse_rules_file(PathBuf::from("sch1-mix-rules.json")).expect("must parse rules");
    let initial = SearchQueueItem {
        drug: Drugs::Cocaine,
        substances: PackedValues::new(),
        effects: Effects::empty(),
    };

    c.bench_function("pareto", |b| {
        b.iter(|| {
            let mut front = FnvHashMap::default();
            search::depth_first_search_pareto(&rules, initial, 5, &mut front);
            let mut top = TopSet::new(5, PartialOrd::gt);
            for (effects, f) in front {
                let min = f.min_objective_1().unwrap();
                top.insert((
                    profit(
                        base_price(initial.drug),
                        min.data.substances.iter(),
                        effects,
                        &rules,
                        999,
                    ),
                    min.data,
                ));
            }
        })
    });
}

criterion_group!(benches, pareto);
criterion_main!(benches);
