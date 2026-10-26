//! Benchmark for `diff`, the call a backend makes on every refresh.
//!
//! Run with `cargo bench -p lutece`. Compare against a saved baseline with
//! `--save-baseline` / `--baseline`; a regression above 5% should fail CI.

// `criterion_group!` expands to an undocumented public function.
#![allow(missing_docs)]

use criterion::{Criterion, criterion_group, criterion_main};
use lutece::{Placement, Rect, Scene, WinId, diff};
use std::hint::black_box;

const WINDOWS: u64 = 64;

/// `shift` moves every window sideways so the diff has real work to do.
fn scene(shift: i32) -> Scene {
    let placements = (0..WINDOWS)
        .map(|n| Placement {
            id: WinId::new(n),
            rect: Rect::new(shift + i32::try_from(n).expect("small"), 0, 100, 100)
                .expect("non-zero size"),
        })
        .collect();
    Scene::try_new(placements, Some(WinId::new(0))).expect("ids are distinct")
}

fn bench_diff(c: &mut Criterion) {
    let (before, after) = (scene(0), scene(5));
    c.bench_function("diff/64 windows, all moved", |b| {
        b.iter(|| diff(black_box(&before), black_box(&after)));
    });
    c.bench_function("diff/64 windows, unchanged", |b| {
        b.iter(|| diff(black_box(&before), black_box(&before)));
    });
}

criterion_group!(benches, bench_diff);
criterion_main!(benches);
