//! RNF-007: memoria y latencia del índice de archivos con 500 000 entradas sintéticas.
//!
//! ```bash
//! cargo run --release -p rukes-plugins --example files_bench
//! ```
#![allow(clippy::unwrap_used, missing_docs)]

use std::time::Instant;

use rukes_core::CancelToken;
use rukes_platform::sysinfo::memory_mb;
use rukes_plugins::files::index::{Index, Kind, NO_PARENT};

const WORDS: &[&str] = &[
    "informe",
    "factura",
    "proyecto",
    "foto",
    "video",
    "notas",
    "presupuesto",
    "contrato",
    "backup",
    "draft",
    "final",
    "cliente",
    "reunion",
    "diseño",
    "logo",
    "datos",
    "export",
    "imagen",
    "captura",
    "plantilla",
    "manual",
    "guia",
    "resumen",
    "tesis",
    "cv",
    "ventas",
];
const EXTS: &[&str] = &[
    "pdf", "docx", "xlsx", "png", "jpg", "mp4", "txt", "zip", "md", "rs",
];

fn main() {
    let total: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500_000);
    let (_, before) = memory_mb();
    let t = Instant::now();
    let mut idx = Index::default();
    let root = idx.push(NO_PARENT, r"C:\Users\bench", true, false);
    let mut dirs = vec![root];
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for i in 1..total {
        let parent = dirs[(rnd() as usize) % dirs.len()];
        let a = WORDS[(rnd() as usize) % WORDS.len()];
        let b = WORDS[(rnd() as usize) % WORDS.len()];
        if i % 10 == 0 {
            dirs.push(idx.push(parent, &format!("{a} {b} {i}"), true, false));
        } else {
            let ext = EXTS[(rnd() as usize) % EXTS.len()];
            idx.push(parent, &format!("{a}_{b}_{i:06}.{ext}"), false, false);
        }
    }
    drop(dirs);
    let build = t.elapsed();
    let (_, after) = memory_mb();
    println!("entradas: {}", idx.len());
    println!("construcción: {:.0} ms", build.as_secs_f64() * 1000.0);
    println!(
        "memoria del índice: {:.1} MB (contabilizada)",
        idx.memory_bytes() as f64 / 1048576.0
    );
    println!(
        "memoria privada extra del proceso: {:.1} MB",
        after - before
    );

    let c = CancelToken::new();
    for q in [
        "inf",
        "presupuesto final",
        "fac 2024",
        "zzzz",
        "dis log",
        "contrato .pdf",
    ] {
        let (text, ext) = rukes_plugins::files::split_ext(q);
        let mut times = Vec::new();
        let mut hits = 0;
        for _ in 0..20 {
            let t = Instant::now();
            hits = idx.search(&text, Kind::Any, ext.as_deref(), 30, &c).len();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "«{q}»: p50 {:.1} ms · p95 {:.1} ms · {hits} resultados",
            times[10], times[18]
        );
    }

    let path = std::env::temp_dir().join("rukes-files-bench.bin");
    let t = Instant::now();
    idx.save(&path, 1).unwrap();
    let save = t.elapsed();
    let size = std::fs::metadata(&path).unwrap().len();
    let t = Instant::now();
    let loaded = Index::load(&path, 1).unwrap();
    println!(
        "caché: {:.1} MB · guardar {:.0} ms · cargar {:.0} ms ({} entradas)",
        size as f64 / 1048576.0,
        save.as_secs_f64() * 1000.0,
        t.elapsed().as_secs_f64() * 1000.0,
        loaded.len()
    );
    std::fs::remove_file(&path).unwrap();
}
