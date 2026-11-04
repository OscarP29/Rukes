//! Coincidencia difusa con `nucleo-matcher` y puntuación normalizada.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Matcher para una consulta. No es `Sync`: se crea uno por consulta y por hilo.
pub struct Fuzzy {
    matcher: Matcher,
    pattern: Pattern,
    perfect: f32,
    buf: Vec<char>,
    empty: bool,
}

impl Fuzzy {
    /// Prepara el patrón. Ignora mayúsculas y acentos (`Normalization::Smart`).
    pub fn new(query: &str) -> Self {
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut buf = Vec::new();
        let perfect = pattern
            .score(Utf32Str::new(query, &mut buf), &mut matcher)
            .unwrap_or(1) as f32;
        Self {
            matcher,
            pattern,
            perfect: perfect.max(1.0),
            buf,
            empty: query.trim().is_empty(),
        }
    }

    /// `true` si la consulta está vacía.
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Puntuación normalizada en `0..=1` o `None` si no coincide. Una consulta vacía coincide
    /// con todo con puntuación 0.
    pub fn score(&mut self, haystack: &str) -> Option<f32> {
        if self.empty {
            return Some(0.0);
        }
        let raw = self
            .pattern
            .score(Utf32Str::new(haystack, &mut self.buf), &mut self.matcher)?;
        Some((raw as f32 / self.perfect).min(1.0))
    }

    /// Mejor puntuación entre varios textos (título, alias, nombre de archivo...).
    pub fn best<'a>(&mut self, haystacks: impl IntoIterator<Item = &'a str>) -> Option<f32> {
        haystacks
            .into_iter()
            .filter_map(|h| self.score(h))
            .fold(None, |acc, s| Some(acc.map_or(s, |a: f32| a.max(s))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_is_one() {
        let mut f = Fuzzy::new("chrome");
        assert_eq!(f.score("chrome"), Some(1.0));
    }

    #[test]
    fn prefix_beats_scattered() {
        let mut f = Fuzzy::new("chr");
        let a = f.score("Google Chrome").unwrap();
        let b = f.score("Character Map Recorder").unwrap();
        assert!(a > b, "{a} <= {b}");
    }

    #[test]
    fn no_match_is_none() {
        let mut f = Fuzzy::new("xyz");
        assert_eq!(f.score("Google Chrome"), None);
    }

    #[test]
    fn ignores_case_and_accents() {
        let mut f = Fuzzy::new("configuracion");
        assert!(f.score("Configuración").is_some());
    }

    #[test]
    fn empty_query_matches_everything() {
        let mut f = Fuzzy::new("  ");
        assert!(f.is_empty());
        assert_eq!(f.score("anything"), Some(0.0));
    }

    #[test]
    fn best_takes_max() {
        let mut f = Fuzzy::new("code");
        let s = f.best(["Visual Studio Code", "code"]).unwrap();
        assert_eq!(s, 1.0);
    }
}
