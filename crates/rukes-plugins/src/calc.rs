//! Calculadora, unidades, temperaturas y fecha/hora (RF-070, RF-071, RF-073, RF-074).
//!
//! Usa `fend-core`. Las consultas en español se traducen antes («5 km a millas» →
//! «5 km to miles», «20% de 340» → «20% of 340»).

use std::time::{Duration, Instant};

use rukes_core::i18n::{Lang, language, tr, trf};
use rukes_core::{
    ActionOutcome, CancelToken, Icon, Plugin, PluginError, PluginMetadata, Query, ResultItem,
};
use rukes_platform::time::{self as ptime, DateTime};

use crate::util::{self, fold, strip_any};

/// Tiempo máximo de evaluación de una expresión.
const BUDGET: Duration = Duration::from_millis(150);

/// Plugin de calculadora.
pub struct CalcPlugin {
    /// Prefijo (`=`).
    pub prefix: String,
    /// Evaluar sin prefijo si la consulta parece una expresión.
    pub auto: bool,
}

struct Deadline<'a> {
    until: Instant,
    cancel: &'a CancelToken,
}

impl fend_core::Interrupt for Deadline<'_> {
    fn should_interrupt(&self) -> bool {
        self.cancel.is_cancelled() || Instant::now() > self.until
    }
}

const UNIT_WORDS: &[(&str, &str)] = &[
    ("kilometros", "km"),
    ("kilometro", "km"),
    ("metros", "m"),
    ("metro", "m"),
    ("centimetros", "cm"),
    ("centimetro", "cm"),
    ("milimetros", "mm"),
    ("millas", "miles"),
    ("milla", "mile"),
    ("pulgadas", "inches"),
    ("pulgada", "inch"),
    ("pies", "feet"),
    ("pie", "foot"),
    ("yardas", "yards"),
    ("libras", "pounds"),
    ("libra", "pound"),
    ("kilogramos", "kg"),
    ("kilos", "kg"),
    ("kilo", "kg"),
    ("gramos", "g"),
    ("onzas", "oz"),
    ("litros", "L"),
    ("litro", "L"),
    ("mililitros", "mL"),
    ("galones", "gallons"),
    ("horas", "hours"),
    ("hora", "hour"),
    ("minutos", "minutes"),
    ("segundos", "seconds"),
    ("dias", "days"),
    ("semanas", "weeks"),
    ("meses", "months"),
    ("anos", "years"),
    ("raiz", "sqrt"),
];

/// Traduce palabras en español a la sintaxis de fend.
pub fn to_fend(q: &str) -> String {
    let words: Vec<&str> = q.split_whitespace().collect();
    let has_digit = q.chars().any(|c| c.is_ascii_digit());
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
        let f = fold(w);
        let prev = if i > 0 { words[i - 1] } else { "" };
        let mapped = match f.as_str() {
            "a" | "en" | "como" if has_digit && i > 0 => "to".to_owned(),
            "de" if prev.ends_with('%') => "of".to_owned(),
            "por" if has_digit => "*".to_owned(),
            "entre" if has_digit => "/".to_owned(),
            _ => {
                // `raiz(144)` o `raiz` suelto.
                let (head, tail) = f.find('(').map_or((f.as_str(), ""), |p| f.split_at(p));
                match UNIT_WORDS.iter().find(|(es, _)| *es == head) {
                    Some((_, en)) => format!("{en}{tail}"),
                    None => (*w).to_owned(),
                }
            }
        };
        out.push(mapped);
    }
    out.join(" ")
}

/// Conversión de temperaturas: `70 F a C`, `-5c en f`, `300 K to C`.
pub fn temperature(q: &str) -> Option<String> {
    let s = fold(q).replace('°', " ").replace("º", " ");
    let toks: Vec<&str> = s.split_whitespace().collect();
    // Formas: [num, from, sep, to] o [numfrom, sep, to].
    let (num, from, to) = match toks.as_slice() {
        [n, f, sep, t] if is_sep(sep) => (*n, *f, *t),
        [nf, sep, t] if is_sep(sep) => {
            let split = nf.find(|c: char| c.is_ascii_alphabetic())?;
            (&nf[..split], &nf[split..], *t)
        }
        _ => return None,
    };
    let v: f64 = num.replace(',', ".").parse().ok()?;
    let unit = |u: &str| match u {
        "c" | "celsius" => Some('C'),
        "f" | "fahrenheit" => Some('F'),
        "k" | "kelvin" => Some('K'),
        _ => None,
    };
    let (from, to) = (unit(from)?, unit(to)?);
    let celsius = match from {
        'C' => v,
        'F' => (v - 32.0) * 5.0 / 9.0,
        _ => v - 273.15,
    };
    let r = match to {
        'C' => celsius,
        'F' => celsius * 9.0 / 5.0 + 32.0,
        _ => celsius + 273.15,
    };
    let r = (r * 100.0).round() / 100.0;
    Some(format!("{r} °{to}").replace("°K", "K"))
}

fn is_sep(s: &str) -> bool {
    matches!(s, "a" | "en" | "to" | "in" | "->")
}

/// Heurística para evaluar sin prefijo: hay un dígito y algo que no sea solo un número.
fn looks_like_math(q: &str) -> bool {
    let q = q.trim();
    if !q.chars().any(|c| c.is_ascii_digit()) || q.parse::<f64>().is_ok() {
        return false;
    }
    q.chars().any(|c| "+-*/^%()×÷".contains(c))
        || q.split_whitespace()
            .any(|w| is_sep(&fold(w)) || fold(w) == "de")
        || fold(q).starts_with("sqrt")
        || fold(q).starts_with("raiz")
}

fn eval(expr: &str, cancel: &CancelToken, preview: bool) -> Option<String> {
    let mut ctx = fend_core::Context::new();
    let int = Deadline {
        until: Instant::now() + BUDGET,
        cancel,
    };
    let res = if preview {
        fend_core::evaluate_preview_with_interrupt(expr, &ctx, &int)
    } else {
        fend_core::evaluate_with_interrupt(expr, &mut ctx, &int).ok()?
    };
    let main = res.get_main_result().trim().trim_start_matches("approx. ");
    (!main.is_empty() && !res.output_is_empty()).then(|| main.to_owned())
}

// ---------------------------------------------------------------------------------------------
// Fecha y hora.

const MONTHS_ES: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];
const MONTHS_EN: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn month_from(word: &str) -> Option<u32> {
    let w = fold(word).trim_end_matches('.').to_owned();
    if w.len() < 3 {
        return None;
    }
    for list in [MONTHS_ES, MONTHS_EN] {
        if let Some(i) = list
            .iter()
            .position(|m| m.starts_with(&w) || w.starts_with(&m[..3]) && m.starts_with(&w[..3]))
        {
            return Some(i as u32 + 1);
        }
    }
    // «set» (septiembre abreviado).
    (w == "set").then_some(9)
}

/// Días desde 1970-01-01 (algoritmo de H. Hinnant).
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(if m <= 2 { y - 1 } else { y });
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Fecha civil a partir de días desde 1970-01-01 (inverso de [`days_from_civil`]).
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y as i32, m, d)
}

fn weekday_name(days: i64) -> &'static str {
    // 1970-01-01 fue jueves.
    let idx = (days + 3).rem_euclid(7) as usize; // 0 = lunes
    match language() {
        Lang::Es => [
            "lunes",
            "martes",
            "miércoles",
            "jueves",
            "viernes",
            "sábado",
            "domingo",
        ][idx],
        Lang::En => [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ][idx],
    }
}

/// Interpreta una fecha: `2026-12-25`, `25 dic`, `25 de diciembre de 2027`, `dec 25`.
/// Sin año, toma la próxima ocurrencia a partir de `today`.
pub fn parse_date(s: &str, today: DateTime) -> Option<(i32, u32, u32)> {
    let s = s.trim().trim_start_matches("el ").trim();
    if let [y, m, d] = s.split('-').collect::<Vec<_>>().as_slice()
        && let (Ok(y), Ok(m), Ok(d)) = (y.parse(), m.parse(), d.parse())
    {
        return valid(y, m, d);
    }
    let toks: Vec<String> = s
        .split(|c: char| c.is_whitespace() || c == '/' || c == ',')
        .map(fold)
        .filter(|t| !t.is_empty() && t != "de" && t != "del" && t != "el" && t != "of")
        .collect();
    let mut day = None;
    let mut month = None;
    let mut year = None;
    for t in &toks {
        if let Ok(n) = t.parse::<u32>() {
            if n > 31 {
                year = Some(n as i32);
            } else if day.is_none() {
                day = Some(n);
            } else if month.is_none() && n <= 12 {
                month = Some(n);
            }
        } else if let Some(m) = month_from(t) {
            month = Some(m);
        } else {
            return None;
        }
    }
    let (d, m) = (day?, month?);
    match year {
        Some(y) => valid(y, m, d),
        None => {
            let this = days_from_civil(today.year, m, d);
            let now = days_from_civil(today.year, today.month, today.day);
            let y = if this < now {
                today.year + 1
            } else {
                today.year
            };
            valid(y, m, d)
        }
    }
}

fn valid(y: i32, m: u32, d: u32) -> Option<(i32, u32, u32)> {
    let dim = [
        31,
        if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    ((1..=12).contains(&m) && d >= 1 && d <= dim[(m - 1) as usize]).then_some((y, m, d))
}

fn fmt_date(y: i32, m: u32, d: u32) -> String {
    let days = days_from_civil(y, m, d);
    match language() {
        Lang::Es => format!(
            "{} {d} de {} de {y}",
            weekday_name(days),
            MONTHS_ES[(m - 1) as usize]
        ),
        Lang::En => {
            let name = MONTHS_EN[(m - 1) as usize];
            let mut c = name.chars();
            let cap = c
                .next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default();
            format!("{}, {cap} {d}, {y}", weekday_name(days))
        }
    }
}

/// Ciudad → zona horaria de Windows.
const CITIES: &[(&str, &str)] = &[
    ("tokio", "Tokyo Standard Time"),
    ("tokyo", "Tokyo Standard Time"),
    ("osaka", "Tokyo Standard Time"),
    ("seul", "Korea Standard Time"),
    ("seoul", "Korea Standard Time"),
    ("pekin", "China Standard Time"),
    ("beijing", "China Standard Time"),
    ("shanghai", "China Standard Time"),
    ("hong kong", "China Standard Time"),
    ("singapur", "Singapore Standard Time"),
    ("singapore", "Singapore Standard Time"),
    ("manila", "Singapore Standard Time"),
    ("bangkok", "SE Asia Standard Time"),
    ("yakarta", "SE Asia Standard Time"),
    ("jakarta", "SE Asia Standard Time"),
    ("delhi", "India Standard Time"),
    ("mumbai", "India Standard Time"),
    ("india", "India Standard Time"),
    ("dubai", "Arabian Standard Time"),
    ("moscu", "Russian Standard Time"),
    ("moscow", "Russian Standard Time"),
    ("estambul", "Turkey Standard Time"),
    ("istanbul", "Turkey Standard Time"),
    ("el cairo", "Egypt Standard Time"),
    ("cairo", "Egypt Standard Time"),
    ("johannesburgo", "South Africa Standard Time"),
    ("johannesburg", "South Africa Standard Time"),
    ("madrid", "Romance Standard Time"),
    ("barcelona", "Romance Standard Time"),
    ("paris", "Romance Standard Time"),
    ("berlin", "W. Europe Standard Time"),
    ("roma", "W. Europe Standard Time"),
    ("rome", "W. Europe Standard Time"),
    ("amsterdam", "W. Europe Standard Time"),
    ("londres", "GMT Standard Time"),
    ("london", "GMT Standard Time"),
    ("lisboa", "GMT Standard Time"),
    ("lisbon", "GMT Standard Time"),
    ("utc", "UTC"),
    ("gmt", "UTC"),
    ("nueva york", "Eastern Standard Time"),
    ("new york", "Eastern Standard Time"),
    ("nyc", "Eastern Standard Time"),
    ("miami", "Eastern Standard Time"),
    ("toronto", "Eastern Standard Time"),
    ("chicago", "Central Standard Time"),
    ("denver", "Mountain Standard Time"),
    ("phoenix", "US Mountain Standard Time"),
    ("los angeles", "Pacific Standard Time"),
    ("san francisco", "Pacific Standard Time"),
    ("seattle", "Pacific Standard Time"),
    ("vancouver", "Pacific Standard Time"),
    ("honolulu", "Hawaiian Standard Time"),
    ("ciudad de mexico", "Central Standard Time (Mexico)"),
    ("mexico", "Central Standard Time (Mexico)"),
    ("bogota", "SA Pacific Standard Time"),
    ("medellin", "SA Pacific Standard Time"),
    ("colombia", "SA Pacific Standard Time"),
    ("lima", "SA Pacific Standard Time"),
    ("quito", "SA Pacific Standard Time"),
    ("panama", "SA Pacific Standard Time"),
    ("caracas", "Venezuela Standard Time"),
    ("santiago", "Pacific SA Standard Time"),
    ("buenos aires", "Argentina Standard Time"),
    ("argentina", "Argentina Standard Time"),
    ("sao paulo", "E. South America Standard Time"),
    ("rio de janeiro", "E. South America Standard Time"),
    ("montevideo", "Montevideo Standard Time"),
    ("sidney", "AUS Eastern Standard Time"),
    ("sydney", "AUS Eastern Standard Time"),
    ("melbourne", "AUS Eastern Standard Time"),
    ("auckland", "New Zealand Standard Time"),
];

fn item(id: &str, title: String, subtitle: String, copy: String) -> ResultItem {
    let mut it = ResultItem::new(id, title);
    it.subtitle = subtitle;
    it.fuzzy = 1.0;
    it.icon = Icon::Named("calc".into());
    it.payload = copy;
    it
}

fn hint() -> &'static str {
    tr("calc.enter_copies")
}

/// Resultados de fecha/hora, si la consulta es de ese tipo.
pub fn datetime(
    q: &str,
    now: impl Fn() -> DateTime,
    in_zone: impl Fn(&str) -> Option<DateTime>,
) -> Option<ResultItem> {
    let f = fold(q.trim());
    if let Some(place) = strip_any(q, &["hora en", "que hora es en", "time in", "hora de"]) {
        let place_f = fold(place);
        let (_, zone) = CITIES.iter().find(|(c, _)| *c == place_f)?;
        let t = in_zone(zone)?;
        let text = format!("{:02}:{:02}", t.hour, t.minute);
        return Some(item(
            "time",
            text.clone(),
            format!(
                "{place} · {} · {}",
                fmt_date(t.year, t.month, t.day),
                hint()
            ),
            text,
        ));
    }
    if let Some(rest) = strip_any(
        q,
        &[
            "dias hasta",
            "dias para",
            "faltan para",
            "days until",
            "days to",
        ],
    ) {
        let today = now();
        let (y, m, d) = parse_date(rest, today)?;
        let diff = days_from_civil(y, m, d) - days_from_civil(today.year, today.month, today.day);
        let text = trf("calc.days", &[("n", &diff)]);
        return Some(item(
            "days",
            text,
            format!("{} · {}", fmt_date(y, m, d), hint()),
            diff.to_string(),
        ));
    }
    if matches!(f.as_str(), "hoy" | "fecha" | "today" | "date") {
        let t = now();
        let iso = format!("{:04}-{:02}-{:02}", t.year, t.month, t.day);
        return Some(item(
            "today",
            fmt_date(t.year, t.month, t.day),
            format!("{iso} · {}", hint()),
            iso,
        ));
    }
    if matches!(f.as_str(), "ahora" | "hora" | "now" | "time") {
        let t = now();
        let text = format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
        return Some(item(
            "now",
            text.clone(),
            format!("{} · {}", fmt_date(t.year, t.month, t.day), hint()),
            text,
        ));
    }
    None
}

impl CalcPlugin {
    /// Evalúa una consulta. `targeted` = escrita con prefijo.
    pub fn compute(&self, text: &str, targeted: bool, cancel: &CancelToken) -> Option<ResultItem> {
        if let Some(it) = datetime(text, ptime::local_now, ptime::now_in_zone) {
            return Some(it);
        }
        let wanted = targeted || (self.auto && looks_like_math(text));
        if !wanted {
            return None;
        }
        if let Some(t) = temperature(text) {
            return Some(item("calc", t.clone(), format!("{text} · {}", hint()), t));
        }
        let expr = to_fend(text);
        let result = eval(&expr, cancel, !targeted)?;
        if !targeted && fold(&result) == fold(text.trim()) {
            return None;
        }
        Some(item(
            "calc",
            result.clone(),
            format!("{text} · {}", hint()),
            result,
        ))
    }
}

impl Plugin for CalcPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "calc".into(),
            name: tr("calc.name").into(),
            prefix: Some(self.prefix.clone()),
            keywords: Vec::new(),
            priority: 0.5,
            global: true,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(
            self.compute(&q.text, q.targeted, cancel)
                .into_iter()
                .collect(),
        );
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        match action {
            rukes_core::ActionId::OPEN => util::copy(&item.payload),
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc() -> CalcPlugin {
        CalcPlugin {
            prefix: "=".into(),
            auto: true,
        }
    }

    fn eval_t(q: &str) -> Option<String> {
        calc()
            .compute(q, true, &CancelToken::new())
            .map(|i| i.title)
    }

    fn eval_auto(q: &str) -> Option<String> {
        calc()
            .compute(q, false, &CancelToken::new())
            .map(|i| i.title)
    }

    #[test]
    fn translates_spanish() {
        assert_eq!(to_fend("5 km a millas"), "5 km to miles");
        assert_eq!(to_fend("20% de 340"), "20% of 340");
        assert_eq!(to_fend("3 horas en minutos"), "3 hours to minutes");
        assert_eq!(to_fend("raiz(144)"), "sqrt(144)");
    }

    #[test]
    fn use_case_2_km_to_miles() {
        let r = eval_t("5 km a millas").unwrap();
        assert!(r.starts_with("3.10"), "{r}");
        assert!(r.contains("mi"), "{r}");
    }

    #[test]
    fn math() {
        assert_eq!(eval_t("2^10").as_deref(), Some("1024"));
        assert_eq!(eval_t("sqrt(144)").as_deref(), Some("12"));
        assert_eq!(eval_t("20% de 340").as_deref(), Some("68"));
    }

    #[test]
    fn temperatures() {
        assert_eq!(temperature("70 F en C").as_deref(), Some("21.11 °C"));
        assert_eq!(temperature("100c a f").as_deref(), Some("212 °F"));
        assert_eq!(temperature("0 C to K").as_deref(), Some("273.15 K"));
        assert_eq!(temperature("70 F"), None);
    }

    #[test]
    fn auto_mode_only_for_math() {
        assert_eq!(eval_auto("2+2").as_deref(), Some("4"));
        assert_eq!(eval_auto("chrome"), None);
        assert_eq!(eval_auto("2026"), None);
        assert_eq!(eval_auto("7zip"), None);
    }

    #[test]
    fn dates() {
        let today = DateTime {
            year: 2026,
            month: 11,
            day: 4,
            hour: 9,
            minute: 0,
            second: 0,
        };
        assert_eq!(parse_date("25 dic", today), Some((2026, 12, 25)));
        assert_eq!(parse_date("25 de diciembre", today), Some((2026, 12, 25)));
        assert_eq!(parse_date("dec 25", today), Some((2026, 12, 25)));
        assert_eq!(parse_date("1 ene", today), Some((2027, 1, 1)));
        assert_eq!(parse_date("2027-03-01", today), Some((2027, 3, 1)));
        assert_eq!(parse_date("31 feb", today), None);
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 12, 25), (1969, 12, 31)] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
        assert_eq!(
            days_from_civil(2026, 12, 25) - days_from_civil(2026, 11, 4),
            51
        );
    }

    #[test]
    fn datetime_queries() {
        let _ = rukes_core::i18n::set_language("es", "");
        let now = || DateTime {
            year: 2026,
            month: 11,
            day: 4,
            hour: 9,
            minute: 5,
            second: 0,
        };
        let zone = |z: &str| {
            (z == "Tokyo Standard Time").then_some(DateTime {
                year: 2026,
                month: 11,
                day: 4,
                hour: 23,
                minute: 5,
                second: 0,
            })
        };
        assert_eq!(datetime("hora en Tokio", now, zone).unwrap().title, "23:05");
        assert_eq!(
            datetime("días hasta 25 dic", now, zone).unwrap().title,
            "51 días"
        );
        assert!(
            datetime("hoy", now, zone)
                .unwrap()
                .title
                .starts_with("miércoles 4 de noviembre")
        );
        assert!(datetime("hora en Narnia", now, zone).is_none());
    }
}
