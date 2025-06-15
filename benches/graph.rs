use bytemuck::cast_slice;
use criterion::{criterion_group, criterion_main, Criterion};
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, ParallelExtend, ParallelIterator,
};
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::time::Duration;
use wide::{u32x4, u32x8};

fn linear_rows(data: &[u8], target: u32, predecessors: &mut Vec<u32>) {
    let view: &[[u32; 16]] = cast_slice(data);
    assert_eq!(
        view.len(),
        1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904 + 5379616 + 18156204
    );
    predecessors.clear();
    for (idx, row) in view.iter().enumerate() {
        let idx = idx as u32;
        if idx == target {
            continue;
        }
        for item in row {
            if *item == target {
                predecessors.push(idx);
            }
        }
    }
}

fn linear_flat(data: &[u8], target: u32, predecessors: &mut Vec<u32>) {
    let view: &[u32] = cast_slice(data);
    assert_eq!(
        view.len(),
        (1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904 + 5379616 + 18156204) * 16
    );
    predecessors.clear();
    for (idx, item) in view.iter().enumerate() {
        let idx = (idx / 16) as u32;
        if idx == target {
            continue;
        }

        if *item == target {
            predecessors.push(idx);
        }
    }
}

fn simd_rows(data: &[u8], target: u32, predecessors: &mut Vec<u32>) {
    let view: &[[u32x4; 4]] = cast_slice(data);
    assert_eq!(
        view.len(),
        1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904 + 5379616 + 18156204
    );
    let wide_target = u32x4::new([target, target, target, target]);
    predecessors.clear();
    for (idx, row) in view.iter().enumerate() {
        let idx = idx as u32;
        if idx == target {
            continue;
        }
        for item in row {
            if u32x4::any(item.cmp_eq(wide_target)) {
                predecessors.push(idx);
            }
        }
    }
}

fn simd_rows_8(data: &[u8], target: u32, predecessors: &mut Vec<u32>) {
    let view: &[[u32x8; 2]] = cast_slice(data);
    assert_eq!(
        view.len(),
        1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904 + 5379616 + 18156204
    );
    let wide_target = u32x8::new([
        target, target, target, target, target, target, target, target,
    ]);
    predecessors.clear();
    for (idx, row) in view.iter().enumerate() {
        let idx = idx as u32;
        if idx == target {
            continue;
        }
        for item in row {
            if u32x8::any(item.cmp_eq(wide_target)) {
                predecessors.push(idx);
            }
        }
    }
}

fn parallel_linear_rows(data: &[u8], target: u32, predecessors: &mut Vec<u32>) {
    let view: &[[u32; 16]] = cast_slice(data);
    predecessors.clear();
    predecessors.par_extend(view.par_iter().enumerate().filter_map(|(idx, row)| {
        let idx = idx as u32;
        if idx == target {
            return None;
        }
        for item in row {
            if *item == target {
                return Some(idx);
            }
        }
        None
    }));
}

fn parallel_simd_rows(data: &[u8], target: u32, predecessors: &mut Vec<u32>) {
    let view: &[[u32x4; 4]] = cast_slice(data);
    let wide_target = u32x4::new([target, target, target, target]);
    predecessors.clear();
    predecessors.par_extend(view.par_iter().enumerate().filter_map(|(idx, row)| {
        let idx = idx as u32;
        if idx == target {
            return None;
        }
        for item in row {
            if u32x4::any(item.cmp_eq(wide_target)) {
                return Some(idx);
            }
        }
        None
    }));
}

fn pred(c: &mut Criterion) -> Result<(), Box<dyn Error>> {
    let mut group = c.benchmark_group("predecessor_search");

    let mut data = Vec::with_capacity(
        (1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904 + 5379616 + 18156204) * 4,
    );
    File::open("sch1-graph.data")?.read_to_end(&mut data)?;

    let target = 10_061_989u32;
    let mut results = Vec::new();
    linear_rows(&data, target, &mut results);
    results.sort();
    println!("results: {:?}", results);

    let mut other = Vec::new();
    linear_flat(&data, target, &mut other);
    other.sort();
    assert_eq!(other, results);

    other.clear();
    simd_rows(&data, target, &mut other);
    other.sort();
    assert_eq!(other, results);

    other.clear();
    simd_rows_8(&data, target, &mut other);
    other.sort();
    assert_eq!(other, results);

    other.clear();
    parallel_linear_rows(&data, target, &mut other);
    other.sort();
    assert_eq!(other, results);

    other.clear();
    parallel_simd_rows(&data, target, &mut other);
    other.sort();
    assert_eq!(other, results);

    group.bench_function("linear rows", |b| {
        b.iter(|| linear_rows(&data, target, &mut results))
    });

    group.bench_function("linear flat", |b| {
        b.iter(|| linear_flat(&data, target, &mut results))
    });

    group.bench_function("simd rows", |b| {
        b.iter(|| simd_rows(&data, target, &mut results))
    });

    group.bench_function("simd-8 rows", |b| {
        b.iter(|| simd_rows(&data, target, &mut results))
    });

    group.bench_function("parallel linear rows", |b| {
        b.iter(|| parallel_linear_rows(&data, target, &mut results))
    });

    group.bench_function("parallel simd rows", |b| {
        b.iter(|| parallel_linear_rows(&data, target, &mut results))
    });

    Ok(())
}

criterion_group! {
    name = predecessor_search;
    config = Criterion::default().without_plots().measurement_time(Duration::from_secs(20));
    targets = pred
}
criterion_main!(predecessor_search);
