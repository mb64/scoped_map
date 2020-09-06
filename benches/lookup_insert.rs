use ahash::RandomState;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hayami::SymbolMap;
use indexmap::IndexMap;
use scoped_map::ScopedMapBase;
use std::collections::HashMap;

pub fn insertion_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("insertion");
    for &count in &[10, 100, 1_000, 10_000, 100_000, 1_000_000] {
        group.throughput(criterion::Throughput::Elements(count as u64));
        group.bench_function(&format!("hayami::SymbolTable ({})", count), |b| {
            b.iter(|| {
                let mut table = hayami::SymbolTable::<usize, usize>::new();
                for key in 1..black_box(count) {
                    table.insert(key, key);
                }
                table
            });
        });
        group.bench_function(&format!("im::HashMap ({})", count), |b| {
            b.iter(|| {
                let mut table = im::HashMap::<usize, usize, RandomState>::default();
                for key in 0..black_box(count) {
                    table.insert(key, key);
                }
                table
            });
        });
        group.bench_function(&format!("im_rc::HashMap ({})", count), |b| {
            b.iter(|| {
                let mut table = im_rc::HashMap::<usize, usize, RandomState>::default();
                for key in 0..black_box(count) {
                    table.insert(key, key);
                }
                table
            });
        });
        group.bench_function(&format!("IndexMap ({})", count), |b| {
            b.iter(|| {
                let mut table = IndexMap::<usize, usize, RandomState>::default();
                for key in 0..black_box(count) {
                    table.insert(key, key);
                }
                table
            });
        });
        group.bench_function(&format!("HashMap ({})", count), |b| {
            b.iter(|| {
                let mut table = HashMap::<usize, usize, RandomState>::default();
                for key in 0..black_box(count) {
                    table.insert(key, key);
                }
                table
            });
        });
        group.bench_function(&format!("ScopedMap ({})", count), |b| {
            b.iter(|| {
                let scoped_map_base = ScopedMapBase::<usize, usize>::new();
                let mut table = scoped_map_base.make_map();
                for key in 0..black_box(count) {
                    table.insert(key, key);
                }
                black_box(table);
            });
        });
    }
}

pub fn lookup_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");
    for &count in &[10, 100, 1_000, 10_000, 100_000, 1_000_000] {
        group.throughput(criterion::Throughput::Elements(count as u64));
        group.bench_function(&format!("hayami::SymbolTable ({})", count), |b| {
            let mut table = hayami::SymbolTable::<usize, usize>::new();
            for key in 1..black_box(count) {
                table.insert(key, key);
            }
            b.iter(|| {
                for key in 1..black_box(count) {
                    assert_eq!(table.get(&key), Some(&key));
                }
            });
        });
        group.bench_function(&format!("im::HashMap ({})", count), |b| {
            let mut table = im::HashMap::<usize, usize, RandomState>::default();
            for key in 0..black_box(count) {
                table.insert(key, key);
            }
            b.iter(|| {
                for key in 1..black_box(count) {
                    assert_eq!(table.get(&key), Some(&key));
                }
            });
        });
        group.bench_function(&format!("im_rc::HashMap ({})", count), |b| {
            let mut table = im_rc::HashMap::<usize, usize, RandomState>::default();
            for key in 0..black_box(count) {
                table.insert(key, key);
            }
            b.iter(|| {
                for key in 1..black_box(count) {
                    assert_eq!(table.get(&key), Some(&key));
                }
            });
        });
        group.bench_function(&format!("IndexMap ({})", count), |b| {
            let mut table = IndexMap::<usize, usize, RandomState>::default();
            for key in 0..black_box(count) {
                table.insert(key, key);
            }
            b.iter(|| {
                for key in 1..black_box(count) {
                    assert_eq!(table.get(&key), Some(&key));
                }
            });
        });
        group.bench_function(&format!("HashMap ({})", count), |b| {
            let mut table = HashMap::<usize, usize, RandomState>::default();
            for key in 0..black_box(count) {
                table.insert(key, key);
            }
            b.iter(|| {
                for key in 1..black_box(count) {
                    assert_eq!(table.get(&key), Some(&key));
                }
            });
        });
        group.bench_function(&format!("ScopedMap ({})", count), |b| {
            let scoped_map_base = ScopedMapBase::<usize, usize>::new();
            let mut table = scoped_map_base.make_map();
            for key in 0..black_box(count) {
                table.insert(key, key);
            }
            b.iter(|| {
                for key in 1..black_box(count) {
                    assert_eq!(table.lookup(&key), Some(&key));
                }
            });
        });
    }
}

fn just_scoped_map(c: &mut Criterion) {
    let mut group = c.benchmark_group("just ScopedMap");
    for &count in &[10, 100, 1_000, 10_000, 100_000, 1_000_000] {
        group.bench_function(&format!("lookup ({})", count), |b| {
            let scoped_map_base = ScopedMapBase::<usize, usize>::new();
            let mut table = scoped_map_base.make_map();
            for key in 0..black_box(count) {
                table.insert(key, key);
            }
            b.iter(|| {
                for key in 1..black_box(count) {
                    assert_eq!(table.lookup(&key), Some(&key));
                }
            });
        });
        group.bench_function(&format!("insertion ({})", count), |b| {
            b.iter(|| {
                let scoped_map_base = ScopedMapBase::<usize, usize>::new();
                let mut table = scoped_map_base.make_map();
                for key in 0..black_box(count) {
                    table.insert(key, key);
                }
                black_box(table);
            });
        });
    }
}

criterion_group!(
    benches,
    insertion_benchmarks,
    lookup_benchmarks,
    just_scoped_map
);
criterion_main!(benches);
