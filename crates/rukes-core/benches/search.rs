//! RNF-004: búsqueda difusa + puntuación + orden sobre 1k/10k/100k elementos.
#![allow(missing_docs, clippy::unwrap_used)]

use criterion::{Criterion, criterion_group, criterion_main};
use rukes_core::{Fuzzy, ResultItem, ranking};

fn corpus(n: usize) -> Vec<String> {
    let words = [
        "Google",
        "Chrome",
        "Visual",
        "Studio",
        "Code",
        "Windows",
        "Terminal",
        "Settings",
        "Paint",
        "Notepad",
        "Explorer",
        "Firefox",
        "Steam",
        "Discord",
        "Spotify",
        "Calculator",
    ];
    (0..n)
        .map(|i| {
            format!(
                "{} {} {} {i}",
                words[i % words.len()],
                words[(i / 7) % words.len()],
                words[(i / 31) % words.len()]
            )
        })
        .collect()
}

fn search(items: &[String], q: &str) -> Vec<ResultItem> {
    let mut f = Fuzzy::new(q);
    let mut out: Vec<ResultItem> = items
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let s = f.score(t)?;
            let mut r = ResultItem::new(i.to_string(), t.clone());
            r.fuzzy = s;
            r.score = s;
            Some(r)
        })
        .collect();
    out.sort_by(ranking::compare);
    out.truncate(50);
    out
}

fn bench(c: &mut Criterion) {
    for n in [1_000, 10_000, 100_000] {
        let items = corpus(n);
        c.bench_function(&format!("fuzzy_{n}_chr"), |b| {
            b.iter(|| search(&items, "chr"))
        });
        c.bench_function(&format!("fuzzy_{n}_vsc"), |b| {
            b.iter(|| search(&items, "vs code"))
        });
    }
}

criterion_group!(benches, bench);
criterion_main!(benches);
