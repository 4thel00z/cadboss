use std::hint::black_box;

use cadboss_core::Budget;
use cadboss_dxf::{DxfDocument, Tape};
use cadboss_testkit::dxf::{multi_layout, realistic_drawing, DxfBuilder};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};

fn inputs() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("realistic_10k", realistic_drawing(10_000)),
        ("realistic_100k", realistic_drawing(100_000)),
        ("multi_layout_50", multi_layout(50)),
    ]
}

fn lex(c: &mut Criterion) {
    let mut group = c.benchmark_group("lex");
    for (name, bytes) in inputs() {
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function(name, |b| {
            b.iter(|| Tape::lex(black_box(&bytes), &Budget::default()).ok())
        });
    }
    group.finish();
}

fn document(c: &mut Criterion) {
    let mut group = c.benchmark_group("document");
    for (name, bytes) in inputs() {
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function(name, |b| {
            b.iter_batched(
                || bytes.clone(),
                |owned| {
                    DxfDocument::load(black_box(owned))
                        .ok()
                        .map(|loaded| loaded.document())
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

fn text(c: &mut Criterion) {
    let mut group = c.benchmark_group("text");
    for (name, bytes) in inputs() {
        let Ok(document) = DxfDocument::load(bytes.clone()) else {
            continue;
        };
        group.throughput(Throughput::Bytes(bytes.len() as u64));
        group.bench_function(name, |b| b.iter(|| black_box(&document).text()));
    }
    group.finish();
}

fn decode(c: &mut Criterion) {
    let corners: Vec<(f64, f64, f64)> = (0..200_000)
        .map(|index| (f64::from(index), f64::from(index % 7), 0.0))
        .collect();
    let bytes = DxfBuilder::new().lwpolyline(&corners, false).build();
    let length = bytes.len() as u64;
    let Ok(document) = DxfDocument::load(bytes) else {
        return;
    };
    let mut group = c.benchmark_group("decode");
    group.throughput(Throughput::Bytes(length));
    group.bench_function("lwpolyline_200k_vertices", |b| {
        b.iter(|| black_box(&document).entities("Model").count())
    });
    group.finish();
}

criterion_group!(benches, lex, document, text, decode);
criterion_main!(benches);
