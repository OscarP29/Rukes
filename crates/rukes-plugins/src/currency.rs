//! Conversión de divisas (RF-072): `100 usd a eur`, `50 dólares en pesos colombianos`.
//!
//! Las tasas se guardan en caché y se actualizan en segundo plano (como mucho dos veces al día)
//! desde open.er-api.com si `currency_online` está activo; las consultas nunca esperan a la red.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, Icon, Plugin, PluginError, PluginMetadata, Query,
    ResultItem, persist,
};
use serde::{Deserialize, Serialize};

use crate::util::{self, ago, now_unix};

/// Origen de las tasas (sin clave; datos diarios).
pub const RATES_URL: &str = "https://open.er-api.com/v6/latest/USD";
/// Antigüedad a partir de la cual se vuelven a pedir.
const STALE_AFTER: u64 = 12 * 3600;

/// Nombres comunes → código ISO 4217.
const NAMES: &[(&str, &str)] = &[
    ("$", "USD"),
    ("dolar", "USD"),
    ("dolares", "USD"),
    ("dollar", "USD"),
    ("dollars", "USD"),
    ("dolares americanos", "USD"),
    ("€", "EUR"),
    ("euro", "EUR"),
    ("euros", "EUR"),
    ("£", "GBP"),
    ("libra", "GBP"),
    ("libras", "GBP"),
    ("pound", "GBP"),
    ("pounds", "GBP"),
    ("¥", "JPY"),
    ("yen", "JPY"),
    ("yenes", "JPY"),
    ("peso colombiano", "COP"),
    ("pesos colombianos", "COP"),
    ("peso mexicano", "MXN"),
    ("pesos mexicanos", "MXN"),
    ("peso argentino", "ARS"),
    ("pesos argentinos", "ARS"),
    ("peso chileno", "CLP"),
    ("pesos chilenos", "CLP"),
    ("real", "BRL"),
    ("reales", "BRL"),
    ("sol", "PEN"),
    ("soles", "PEN"),
    ("bolivar", "VES"),
    ("bolivares", "VES"),
    ("franco suizo", "CHF"),
    ("francos suizos", "CHF"),
    ("yuan", "CNY"),
    ("yuanes", "CNY"),
    ("rublo", "RUB"),
    ("rublos", "RUB"),
];

/// Tasas respecto al dólar.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Rates {
    /// Unix de la última actualización del proveedor.
    pub updated: u64,
    /// Código → unidades por 1 USD.
    pub rates: BTreeMap<String, f64>,
}

impl Rates {
    /// Convierte `amount` de `from` a `to`.
    pub fn convert(&self, amount: f64, from: &str, to: &str) -> Option<f64> {
        let (f, t) = (self.rates.get(from)?, self.rates.get(to)?);
        (*f > 0.0).then(|| amount / f * t)
    }

    /// Lee la respuesta de open.er-api.com.
    pub fn from_api(body: &[u8]) -> Option<Self> {
        #[derive(Deserialize)]
        struct Api {
            result: String,
            time_last_update_unix: u64,
            rates: BTreeMap<String, f64>,
        }
        let a: Api = serde_json::from_slice(body).ok()?;
        (a.result == "success" && !a.rates.is_empty()).then_some(Self {
            updated: a.time_last_update_unix,
            rates: a.rates,
        })
    }
}

fn fold(s: &str) -> String {
    util::fold(s.trim())
}

/// Código de una divisa escrita como código (`usd`) o nombre (`dólares`).
fn code_of(name: &str, rates: &Rates) -> Option<String> {
    let n = fold(name);
    if n.len() == 3 && n.chars().all(|c| c.is_ascii_alphabetic()) {
        let c = n.to_ascii_uppercase();
        return rates.rates.contains_key(&c).then_some(c);
    }
    NAMES
        .iter()
        .find(|(k, _)| fold(k) == n)
        .map(|(_, c)| (*c).to_owned())
}

fn parse_amount(s: &str) -> Option<f64> {
    let s = s.trim().replace('_', "");
    // `1.234,5` o `1234,5` (coma decimal) y `1,234.5` o `1234.5` (punto decimal).
    let normalized = match (s.rfind(','), s.rfind('.')) {
        (Some(c), Some(d)) if c > d => s.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => s.replace(',', ""),
        (Some(c), None) if s.len() - c - 1 != 3 => s.replace(',', "."),
        (Some(_), None) => s.replace(',', ""),
        _ => s,
    };
    normalized.parse().ok().filter(|v: &f64| v.is_finite())
}

/// `100 usd a eur` → `(100, "USD", "EUR")`.
pub fn parse_query(text: &str, rates: &Rates) -> Option<(f64, String, String)> {
    let t = text.trim().to_lowercase();
    let words: Vec<&str> = t.split_whitespace().collect();
    let sep = words
        .iter()
        .position(|w| matches!(*w, "a" | "en" | "to" | "in" | "=" | "->"))?;
    let (left, right) = (&words[..sep], &words[sep + 1..]);
    if left.is_empty() || right.is_empty() {
        return None;
    }
    // La cantidad puede ir pegada al símbolo o al código: `$100`, `100usd`.
    let first = left[0];
    let split = first
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '_'))
        .unwrap_or(first.len());
    let (num, sym, rest) = if split == 0 {
        // `$100 a eur`
        let start = first.find(|c: char| c.is_ascii_digit())?;
        (&first[start..], &first[..start], &left[1..])
    } else {
        (&first[..split], &first[split..], &left[1..])
    };
    let amount = parse_amount(num)?;
    let from_name = if sym.is_empty() {
        rest.join(" ")
    } else {
        std::iter::once(sym)
            .chain(rest.iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let from = code_of(&from_name, rates)?;
    let to = code_of(&right.join(" "), rates)?;
    Some((amount, from, to))
}

/// Número con separadores según el idioma: `1.234,57` o `1,234.57`.
pub fn format_amount(v: f64) -> String {
    let decimals = if v.abs() >= 1.0 || v == 0.0 { 2 } else { 6 };
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let es = rukes_core::i18n::language() == rukes_core::i18n::Lang::Es;
    let (group, dec) = if es { ('.', ',') } else { (',', '.') };
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(group);
        }
        out.push(ch);
    }
    let frac = frac.trim_end_matches('0');
    if !frac.is_empty() {
        out.push(dec);
        out.push_str(frac);
    }
    if v < 0.0 { format!("-{out}") } else { out }
}

/// Plugin de divisas.
pub struct CurrencyPlugin {
    cache_file: PathBuf,
    online: bool,
    rates: RwLock<Rates>,
}

impl CurrencyPlugin {
    /// Crea el plugin con las tasas de la caché.
    pub fn new(cache_file: PathBuf, online: bool) -> Self {
        let rates = persist::load_json::<Rates>(&cache_file).unwrap_or_default();
        Self {
            cache_file,
            online,
            rates: RwLock::new(rates),
        }
    }

    /// Con tasas dadas (tests).
    pub fn with_rates(rates: Rates) -> Self {
        Self {
            cache_file: PathBuf::new(),
            online: false,
            rates: RwLock::new(rates),
        }
    }

    /// Resultados para un texto.
    pub fn search(&self, text: &str) -> Vec<ResultItem> {
        let rates = self.rates.read().map(|r| r.clone()).unwrap_or_default();
        // Sin tasas todavía: se reconoce la consulta con los nombres conocidos para avisar.
        let probe = if rates.rates.is_empty() {
            Rates {
                updated: 0,
                rates: NAMES.iter().map(|(_, c)| ((*c).to_owned(), 1.0)).collect(),
            }
        } else {
            rates.clone()
        };
        let Some((amount, from, to)) = parse_query(text, &probe) else {
            return Vec::new();
        };
        if rates.rates.is_empty() {
            let mut it = ResultItem::new("currency:none", tr("currency.no_rates"));
            it.subtitle = if self.online {
                tr("currency.no_rates_online").to_owned()
            } else {
                tr("currency.no_rates_offline").to_owned()
            };
            it.icon = Icon::Named("calc".into());
            it.fuzzy = 1.0;
            it.actions = vec![];
            return vec![it];
        }
        let Some(v) = rates.convert(amount, &from, &to) else {
            return Vec::new();
        };
        let value = format_amount(v);
        let mut it = ResultItem::new("currency", format!("{value} {to}"));
        it.subtitle = trf(
            "currency.subtitle",
            &[
                ("amount", &format_amount(amount)),
                ("from", &from),
                (
                    "rate",
                    &format_amount(rates.convert(1.0, &from, &to).unwrap_or(0.0)),
                ),
                ("to", &to),
                ("when", &ago(rates.updated)),
            ],
        );
        it.icon = Icon::Named("calc".into());
        it.fuzzy = 1.0;
        it.payload = format!("{v:.2}");
        vec![it]
    }

    fn update(&self) {
        let stale = self
            .rates
            .read()
            .map(|r| now_unix().saturating_sub(r.updated) > STALE_AFTER)
            .unwrap_or(true);
        if !self.online || !stale {
            return;
        }
        let req = rukes_platform::http::Request {
            method: "GET",
            url: RATES_URL,
            headers: &[],
            body: &[],
            timeout: Duration::from_secs(10),
            max_bytes: 256 * 1024,
        };
        match rukes_platform::http::send(&req) {
            Ok(r) if r.status == 200 => match Rates::from_api(&r.body) {
                Some(rates) => {
                    tracing::info!(n = rates.rates.len(), "tasas de cambio actualizadas");
                    if let Err(e) = persist::save_json(&self.cache_file, &rates) {
                        tracing::warn!(error = %e, "no se pudieron guardar las tasas");
                    }
                    if let Ok(mut w) = self.rates.write() {
                        *w = rates;
                    }
                }
                None => tracing::warn!("respuesta de tasas no válida"),
            },
            Ok(r) => tracing::warn!(status = r.status, "no se pudieron pedir las tasas"),
            Err(e) => tracing::warn!(error = %e, "no se pudieron pedir las tasas"),
        }
    }
}

impl Plugin for CurrencyPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "currency".into(),
            name: tr("currency.name").into(),
            prefix: None,
            keywords: Vec::new(),
            priority: 0.5,
            global: true,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(self.search(&q.text));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        match action {
            ActionId::OPEN if !item.payload.is_empty() => util::copy(&item.payload),
            ActionId::OPEN => Ok(ActionOutcome::KeepOpen),
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }

    /// Se llama al arrancar y cada cierto tiempo: actualiza las tasas si son viejas.
    fn refresh(&self) {
        self.update();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates() -> Rates {
        Rates {
            updated: now_unix() - 3600,
            rates: [("USD", 1.0), ("EUR", 0.9), ("COP", 4000.0), ("MXN", 18.0)]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), *v))
                .collect(),
        }
    }

    #[test]
    fn parses_queries() {
        let r = rates();
        assert_eq!(
            parse_query("100 usd a eur", &r),
            Some((100.0, "USD".into(), "EUR".into()))
        );
        assert_eq!(
            parse_query("50 dólares en pesos colombianos", &r),
            Some((50.0, "USD".into(), "COP".into()))
        );
        assert_eq!(
            parse_query("$20 to mxn", &r),
            Some((20.0, "USD".into(), "MXN".into()))
        );
        assert_eq!(
            parse_query("1.500,5 eur a usd", &r),
            Some((1500.5, "EUR".into(), "USD".into()))
        );
        assert_eq!(parse_query("100usd a cop", &r).map(|q| q.0), Some(100.0));
        assert_eq!(parse_query("5 km a millas", &r), None);
        assert_eq!(parse_query("100 xyz a eur", &r), None);
    }

    #[test]
    fn converts_and_formats() {
        let _ = rukes_core::i18n::set_language("es", "");
        let p = CurrencyPlugin::with_rates(rates());
        let r = p.search("100 usd a cop");
        assert_eq!(r[0].title, "400.000 COP");
        assert_eq!(r[0].payload, "400000.00");
        let r = p.search("90 eur a usd");
        assert_eq!(r[0].title, "100 USD");
        assert_eq!(format_amount(1234.5), "1.234,5");
        assert_eq!(format_amount(0.000123), "0,000123");
        assert!(p.search("hola a todos").is_empty());
    }

    #[test]
    fn warns_without_rates() {
        let _ = rukes_core::i18n::set_language("es", "");
        let p = CurrencyPlugin::with_rates(Rates::default());
        let r = p.search("10 euros a dolares");
        assert_eq!(r[0].id, "currency:none");
    }

    #[test]
    fn reads_api_response() {
        let body = br#"{"result":"success","time_last_update_unix":1700000000,"rates":{"USD":1,"EUR":0.92}}"#;
        let r = Rates::from_api(body).unwrap();
        assert_eq!(r.updated, 1_700_000_000);
        assert_eq!(r.rates["EUR"], 0.92);
        assert!(Rates::from_api(br#"{"result":"error"}"#).is_none());
    }
}
