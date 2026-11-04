//! Puntuación final y orden estable de resultados (sección 7.3).

use std::cmp::Ordering;

use crate::ResultItem;

/// Bonus por coincidencia exacta con un alias.
pub const ALIAS_BONUS: f32 = 1.0;
/// Peso de la frecency específica de la consulta respecto a la general.
const QUERY_FRECENCY_SHARE: f32 = 0.6;

/// Entradas para calcular la puntuación final.
#[derive(Debug, Clone, Copy)]
pub struct ScoreInput {
    /// Puntuación difusa normalizada (0..1).
    pub fuzzy: f32,
    /// Frecency general del resultado (0..1).
    pub item_frecency: f32,
    /// Frecency para esta consulta exacta (0..1).
    pub query_frecency: f32,
    /// Coincidencia exacta con alias.
    pub alias_match: bool,
    /// Prioridad del plugin.
    pub priority: f32,
}

/// `fuzzy·(1−w) + frecency·w + alias + prioridad`, donde la frecency mezcla la general y la de
/// la consulta exacta.
pub fn final_score(i: ScoreInput, frecency_weight: f32) -> f32 {
    let w = frecency_weight.clamp(0.0, 1.0);
    let frecency =
        i.item_frecency * (1.0 - QUERY_FRECENCY_SHARE) + i.query_frecency * QUERY_FRECENCY_SHARE;
    let alias = if i.alias_match { ALIAS_BONUS } else { 0.0 };
    i.fuzzy * (1.0 - w) + frecency * w + alias + i.priority
}

/// Orden: puntuación descendente; empates por título y luego por clave, para que la lista no
/// «baile» entre pulsaciones.
pub fn compare(a: &ResultItem, b: &ResultItem) -> Ordering {
    b.score
        .total_cmp(&a.score)
        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        .then_with(|| a.plugin_id.cmp(&b.plugin_id))
        .then_with(|| a.id.cmp(&b.id))
}

/// Inserta `incoming` en `list` (ya ordenada), sustituyendo duplicados por clave, y trunca.
pub fn merge(list: &mut Vec<ResultItem>, incoming: Vec<ResultItem>, limit: usize) {
    for item in incoming {
        if let Some(pos) = list
            .iter()
            .position(|x| x.plugin_id == item.plugin_id && x.id == item.id)
        {
            list.remove(pos);
        }
        let at = list.partition_point(|x| compare(x, &item) == Ordering::Less);
        list.insert(at, item);
    }
    list.truncate(limit);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(fuzzy: f32) -> ScoreInput {
        ScoreInput {
            fuzzy,
            item_frecency: 0.0,
            query_frecency: 0.0,
            alias_match: false,
            priority: 0.0,
        }
    }

    fn item(id: &str, title: &str, score: f32) -> ResultItem {
        let mut r = ResultItem::new(id, title);
        r.plugin_id = "t".into();
        r.score = score;
        r
    }

    #[test]
    fn frecency_can_overtake_slightly_better_match() {
        let w = 0.35;
        let used = final_score(
            ScoreInput {
                item_frecency: 0.7,
                query_frecency: 0.7,
                ..input(0.8)
            },
            w,
        );
        let unused = final_score(input(0.9), w);
        assert!(used > unused);
    }

    #[test]
    fn alias_wins() {
        let w = 0.35;
        let alias = final_score(
            ScoreInput {
                alias_match: true,
                ..input(0.3)
            },
            w,
        );
        let perfect = final_score(
            ScoreInput {
                item_frecency: 1.0,
                query_frecency: 1.0,
                ..input(1.0)
            },
            w,
        );
        assert!(alias > perfect);
    }

    #[test]
    fn ties_break_by_title() {
        let mut v = [item("2", "Beta", 1.0), item("1", "alpha", 1.0)];
        v.sort_by(compare);
        assert_eq!(v[0].title, "alpha");
    }

    #[test]
    fn merge_keeps_order_dedups_and_truncates() {
        let mut list = vec![item("a", "A", 0.9), item("b", "B", 0.5)];
        merge(
            &mut list,
            vec![
                item("c", "C", 0.7),
                item("b", "B", 0.95),
                item("d", "D", 0.1),
            ],
            3,
        );
        let ids: Vec<_> = list.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["b", "a", "c"]);
    }
}
