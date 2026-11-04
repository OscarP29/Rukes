//! Frecency: frecuencia de uso con decaimiento exponencial (RF-011).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Versión del formato persistido.
pub const VERSION: u32 = 1;
/// Entradas máximas por mapa antes de podar.
const MAX_ENTRIES: usize = 2_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
struct Entry {
    /// Valor acumulado en el instante `last`.
    value: f64,
    /// Segundos Unix del último uso.
    last: u64,
}

/// Registro de usos por resultado y por par (consulta, resultado).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frecency {
    version: u32,
    /// Vida media del decaimiento en segundos.
    half_life_secs: f64,
    items: HashMap<String, Entry>,
    /// Clave: `consulta_normalizada \u{1f} clave_resultado`.
    queries: HashMap<String, Entry>,
}

impl Default for Frecency {
    fn default() -> Self {
        Self::new(7.0 * 24.0 * 3600.0)
    }
}

fn decayed(e: &Entry, now: u64, half_life: f64) -> f64 {
    let dt = now.saturating_sub(e.last) as f64;
    e.value * 0.5f64.powf(dt / half_life)
}

/// Convierte un valor acumulado (0..∞) en 0..1. 1 uso ≈ 0,22; 5 usos ≈ 0,71; 10 ≈ 0,92.
fn squash(v: f64) -> f32 {
    (1.0 - (-v / 4.0).exp()) as f32
}

fn query_key(query: &str, item_key: &str) -> String {
    format!("{}\u{1f}{item_key}", crate::query::normalize(query))
}

impl Frecency {
    /// Crea un registro vacío con la vida media indicada (segundos).
    pub fn new(half_life_secs: f64) -> Self {
        Self {
            version: VERSION,
            half_life_secs: half_life_secs.max(1.0),
            items: HashMap::new(),
            queries: HashMap::new(),
        }
    }

    /// `true` si el formato cargado es compatible.
    pub fn is_compatible(&self) -> bool {
        self.version == VERSION
    }

    /// Cambia la vida media sin perder datos.
    pub fn set_half_life(&mut self, secs: f64) {
        self.half_life_secs = secs.max(1.0);
    }

    /// Registra que se usó `item_key` tras escribir `query`.
    pub fn record(&mut self, query: &str, item_key: &str, now: u64) {
        let hl = self.half_life_secs;
        let bump = |map: &mut HashMap<String, Entry>, key: String| {
            let e = map.entry(key).or_insert(Entry {
                value: 0.0,
                last: now,
            });
            e.value = decayed(e, now, hl) + 1.0;
            e.last = now;
        };
        bump(&mut self.items, item_key.to_owned());
        if !query.trim().is_empty() {
            bump(&mut self.queries, query_key(query, item_key));
        }
        self.prune(now);
    }

    /// Puntuación de uso general del resultado (0..1).
    pub fn item_score(&self, item_key: &str, now: u64) -> f32 {
        self.items
            .get(item_key)
            .map_or(0.0, |e| squash(decayed(e, now, self.half_life_secs)))
    }

    /// Puntuación del resultado para esta consulta exacta (0..1).
    pub fn query_score(&self, query: &str, item_key: &str, now: u64) -> f32 {
        self.queries
            .get(&query_key(query, item_key))
            .map_or(0.0, |e| squash(decayed(e, now, self.half_life_secs)))
    }

    /// Número de resultados con historial.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` si no hay historial.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn prune(&mut self, now: u64) {
        let hl = self.half_life_secs;
        for map in [&mut self.items, &mut self.queries] {
            if map.len() <= MAX_ENTRIES {
                continue;
            }
            let mut v: Vec<(String, f64)> = map
                .iter()
                .map(|(k, e)| (k.clone(), decayed(e, now, hl)))
                .collect();
            v.sort_by(|a, b| b.1.total_cmp(&a.1));
            for (k, _) in v.into_iter().skip(MAX_ENTRIES * 3 / 4) {
                map.remove(&k);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 24 * 3600;

    #[test]
    fn unused_is_zero() {
        let f = Frecency::default();
        assert_eq!(f.item_score("apps:x", 0), 0.0);
    }

    #[test]
    fn more_uses_score_higher() {
        let mut f = Frecency::default();
        f.record("c", "apps:a", 0);
        for _ in 0..5 {
            f.record("c", "apps:b", 0);
        }
        assert!(f.item_score("apps:b", 0) > f.item_score("apps:a", 0));
    }

    #[test]
    fn decays_by_half_life() {
        let mut f = Frecency::new(DAY as f64);
        for _ in 0..4 {
            f.record("", "apps:a", 0);
        }
        let now = f.item_score("apps:a", 0);
        let later = f.item_score("apps:a", DAY);
        // 4 usos → valor 4; tras una vida media → 2.
        assert!((now - squash(4.0)).abs() < 1e-6);
        assert!((later - squash(2.0)).abs() < 1e-6);
    }

    #[test]
    fn recent_beats_old_frequent() {
        let mut f = Frecency::new(7.0 * DAY as f64);
        for _ in 0..3 {
            f.record("", "apps:old", 0);
        }
        f.record("", "apps:new", 60 * DAY);
        assert!(f.item_score("apps:new", 60 * DAY) > f.item_score("apps:old", 60 * DAY));
    }

    #[test]
    fn query_score_is_query_specific() {
        let mut f = Frecency::default();
        f.record("Chr ", "apps:chrome", 0);
        assert!(f.query_score("chr", "apps:chrome", 0) > 0.0);
        assert_eq!(f.query_score("code", "apps:chrome", 0), 0.0);
    }

    #[test]
    fn prunes_to_bounded_size() {
        let mut f = Frecency::default();
        for i in 0..(MAX_ENTRIES + 10) {
            f.record("", &format!("apps:{i}"), i as u64);
        }
        assert!(f.len() <= MAX_ENTRIES);
    }

    #[test]
    fn roundtrips_json() {
        let mut f = Frecency::default();
        f.record("chr", "apps:chrome", 10);
        let s = serde_json::to_string(&f).unwrap();
        let g: Frecency = serde_json::from_str(&s).unwrap();
        assert!(g.is_compatible());
        assert_eq!(
            g.item_score("apps:chrome", 10),
            f.item_score("apps:chrome", 10)
        );
    }
}
