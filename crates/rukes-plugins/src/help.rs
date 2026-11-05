//! `help` / `info` / `ayuda`: lista los comandos y muestra la ayuda de uso de cada uno
//! (`help vol`, `info calc`). Enter en un ejemplo lo escribe en el launcher para probarlo.

use std::sync::Arc;

use rukes_core::i18n::tr;
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginHelp,
    PluginMetadata, Query, ResultItem,
};

/// Palabras clave.
pub const KEYWORDS: [&str; 3] = ["help", "info", "ayuda"];
/// Disparadores que se muestran como mucho por comando.
const MAX_TRIGGERS: usize = 6;

/// Ayuda de un plugin instalado.
#[derive(Debug, Clone)]
pub struct HelpEntry {
    /// Id del plugin.
    pub id: String,
    /// Nombre visible.
    pub name: String,
    /// Prefijo y palabras clave con que se usa.
    pub triggers: Vec<String>,
    /// Ayuda.
    pub help: PluginHelp,
}

impl HelpEntry {
    /// La ayuda de un plugin, o `None` si no tiene.
    pub fn of(p: &dyn Plugin) -> Option<Self> {
        let meta = p.metadata();
        let help = p.help()?;
        let triggers = meta
            .prefix
            .iter()
            .chain(meta.keywords.iter())
            .filter(|t| !t.trim().is_empty())
            .take(MAX_TRIGGERS)
            .cloned()
            .collect();
        Some(Self {
            id: meta.id,
            name: meta.name,
            triggers,
            help,
        })
    }

    /// `¿text` nombra exactamente este comando?
    fn is(&self, text: &str) -> bool {
        let t = text.trim().to_lowercase();
        let bare = |s: &str| s.trim().trim_end_matches(':').to_lowercase();
        t == self.id.to_lowercase()
            || t == self.name.to_lowercase()
            || self
                .triggers
                .iter()
                .any(|k| k.to_lowercase() == t || bare(k) == bare(&t))
    }

    /// Lo que se escribe tras `help` para ver esta ayuda.
    fn key(&self) -> String {
        self.triggers
            .first()
            .map_or_else(|| self.id.clone(), |t| t.trim().to_owned())
    }
}

/// Plugin de ayuda. Se construye con las ayudas de los plugins instalados.
pub struct HelpPlugin {
    entries: Vec<HelpEntry>,
}

impl HelpPlugin {
    /// Recoge la ayuda de `plugins` (los que no tienen ayuda no aparecen) y la suya propia.
    pub fn new(plugins: &[Arc<dyn Plugin>]) -> Self {
        let mut entries: Vec<HelpEntry> = plugins
            .iter()
            .filter_map(|p| HelpEntry::of(p.as_ref()))
            .collect();
        let me = Self {
            entries: Vec::new(),
        };
        if let Some(own) = HelpEntry::of(&me) {
            entries.push(own);
        }
        entries.sort_by_key(|e| e.name.to_lowercase());
        entries.dedup_by(|a, b| a.id == b.id);
        Self { entries }
    }

    /// Ayudas recogidas.
    pub fn entries(&self) -> &[HelpEntry] {
        &self.entries
    }

    fn overview(e: &HelpEntry, kw: &str, score: f32) -> ResultItem {
        let mut it = ResultItem::new(format!("p:{}", e.id), e.name.clone());
        it.subtitle = if e.triggers.is_empty() {
            e.help.summary.clone()
        } else {
            format!("{}  ·  {}", e.help.summary, e.triggers.join(", "))
        };
        it.icon = Icon::Named("help".into());
        it.fuzzy = score;
        it.payload = format!("{kw} {}", e.key());
        it.detail = Some(e.help.to_markdown(&e.name));
        it
    }

    /// Ayuda completa: cabecera, un resultado por ejemplo y las notas.
    fn details(e: &HelpEntry) -> Vec<ResultItem> {
        let mut out = Vec::new();
        let mut head = ResultItem::new(format!("h:{}", e.id), e.name.clone());
        head.subtitle = e.help.summary.clone();
        head.icon = Icon::Named("help".into());
        head.fuzzy = 1.0;
        head.payload = e
            .help
            .usage
            .first()
            .map(|u| u.example.clone())
            .unwrap_or_else(|| format!("{} ", e.key()));
        head.detail = Some(e.help.to_markdown(&e.name));
        out.push(head);
        // Puntuaciones decrecientes: se conserva el orden de la ayuda.
        let mut score = 0.99;
        for (i, u) in e.help.usage.iter().enumerate() {
            let mut it = ResultItem::new(format!("ex:{}:{i}", e.id), u.example.clone());
            it.subtitle = if u.description.is_empty() {
                tr("help.try").to_owned()
            } else {
                format!("{}  ·  {}", u.description, tr("help.try"))
            };
            it.icon = Icon::Named("shell".into());
            it.fuzzy = score;
            it.payload = u.example.clone();
            out.push(it);
            score -= 0.001;
        }
        for (i, t) in e.help.tips.iter().enumerate() {
            let mut it = ResultItem::new(format!("tip:{}:{i}", e.id), t.clone());
            it.subtitle = tr("help.tip").to_owned();
            it.icon = Icon::Named("ok".into());
            it.fuzzy = score;
            it.actions.clear();
            out.push(it);
            score -= 0.001;
        }
        out
    }

    /// Resultados para el texto tras la palabra clave.
    pub fn search(&self, kw: &str, text: &str) -> Vec<ResultItem> {
        let text = text.trim();
        if text.is_empty() {
            return self
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| Self::overview(e, kw, 1.0 - i as f32 * 0.001))
                .collect();
        }
        if let Some(e) = self.entries.iter().find(|e| e.is(text)) {
            return Self::details(e);
        }
        let mut f = Fuzzy::new(text);
        let mut found: Vec<(f32, &HelpEntry)> = self
            .entries
            .iter()
            .filter_map(|e| {
                let best = std::iter::once(e.name.as_str())
                    .chain(e.triggers.iter().map(String::as_str))
                    .chain(std::iter::once(e.id.as_str()))
                    .filter_map(|s| f.score(s))
                    .fold(0.0f32, f32::max);
                let summary = f.score(&e.help.summary).unwrap_or(0.0) * 0.7;
                let s = best.max(summary);
                (s > 0.0).then_some((s, e))
            })
            .collect();
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        if let [(_, e)] = found.as_slice() {
            return Self::details(e);
        }
        found
            .into_iter()
            .map(|(s, e)| Self::overview(e, kw, s))
            .collect()
    }
}

impl Plugin for HelpPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "help".into(),
            name: tr("help.name").into(),
            prefix: None,
            keywords: KEYWORDS.iter().map(|k| (*k).to_owned()).collect(),
            priority: 0.4,
            global: false,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let kw = q
            .raw
            .split_whitespace()
            .next()
            .map(str::to_lowercase)
            .filter(|w| KEYWORDS.contains(&w.as_str()))
            .unwrap_or_else(|| KEYWORDS[0].to_owned());
        sink(self.search(&kw, &q.text));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if action != ActionId::OPEN {
            return Err(PluginError::UnsupportedAction(action.into()));
        }
        // Lista → abre la ayuda del comando; ejemplo o cabecera → lo escribe para probarlo.
        if item.payload.is_empty() {
            Ok(ActionOutcome::KeepOpen)
        } else {
            Ok(ActionOutcome::ReplaceQuery(item.payload.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rukes_core::HelpUsage;

    struct Fake(&'static str, &'static [&'static str], Option<PluginHelp>);

    impl Plugin for Fake {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                id: self.0.into(),
                name: self.0.to_uppercase(),
                prefix: None,
                keywords: self.1.iter().map(|k| (*k).to_owned()).collect(),
                priority: 0.0,
                global: false,
                list_on_keyword: false,
            }
        }
        fn query(&self, _: &Query, _: &CancelToken, _: &mut dyn FnMut(Vec<ResultItem>)) {}
        fn execute(&self, _: &ResultItem, _: &str) -> Result<ActionOutcome, PluginError> {
            Ok(ActionOutcome::KeepOpen)
        }
        fn help(&self) -> Option<PluginHelp> {
            self.2.clone()
        }
    }

    fn help(summary: &str, examples: &[(&str, &str)]) -> Option<PluginHelp> {
        Some(PluginHelp {
            summary: summary.into(),
            usage: examples
                .iter()
                .map(|(e, d)| HelpUsage {
                    example: (*e).into(),
                    description: (*d).into(),
                })
                .collect(),
            tips: vec!["Un truco".into()],
        })
    }

    fn plugin() -> HelpPlugin {
        let plugins: Vec<Arc<dyn Plugin>> = vec![
            Arc::new(Fake(
                "controls",
                &["vol", "brillo"],
                help("Volumen y brillo", &[("vol 50", "Volumen al 50 %")]),
            )),
            Arc::new(Fake(
                "http",
                &["http", "req"],
                help("Peticiones HTTP", &[("http 3000", "GET a localhost")]),
            )),
            Arc::new(Fake("sinayuda", &["x"], None)),
        ];
        HelpPlugin::new(&plugins)
    }

    #[test]
    fn lists_every_command_with_help() {
        let p = plugin();
        let ids: Vec<&str> = p.entries().iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"controls") && ids.contains(&"http") && ids.contains(&"help"));
        assert!(!ids.contains(&"sinayuda"));
        let all = p.search("info", "");
        assert_eq!(all.len(), p.entries().len());
        let ctl = all.iter().find(|i| i.id == "p:controls").unwrap();
        assert_eq!(ctl.payload, "info vol");
        assert!(ctl.subtitle.contains("vol, brillo"));
    }

    #[test]
    fn exact_keyword_shows_details_and_examples_can_be_tried() {
        let p = plugin();
        let d = p.search("help", "VOL");
        assert_eq!(d[0].id, "h:controls");
        assert!(d[0].detail.as_deref().unwrap().contains("`vol 50`"));
        assert_eq!(d[1].title, "vol 50");
        assert!(matches!(
            p.execute(&d[1], ActionId::OPEN).unwrap(),
            ActionOutcome::ReplaceQuery(q) if q == "vol 50"
        ));
        let tip = d.last().unwrap();
        assert_eq!(tip.title, "Un truco");
        assert!(tip.actions.is_empty());
        // Por id o por otra palabra clave.
        assert_eq!(p.search("help", "req")[0].id, "h:http");
        assert_eq!(p.search("help", "controls")[0].id, "h:controls");
    }

    #[test]
    fn every_builtin_plugin_has_help() {
        for id in [
            "apps",
            "files",
            "calc",
            "currency",
            "color",
            "controls",
            "web",
            "shell",
            "syscmd",
            "windows",
            "processes",
            "generators",
            "clipboard",
            "snippets",
            "notes",
            "todo",
            "remind",
            "workflows",
            "wall",
            "plugins",
            "scripts",
            "rukes",
            "http",
            "help",
        ] {
            let h = PluginHelp::from_i18n(id).unwrap_or_else(|| panic!("sin ayuda: {id}"));
            assert!(!h.usage.is_empty(), "{id} sin ejemplos");
        }
    }

    #[test]
    fn fuzzy_search_lists_candidates() {
        let p = plugin();
        let r = p.search("help", "peticiones");
        assert_eq!(r[0].id, "h:http", "un solo candidato abre su ayuda");
        assert!(p.search("help", "zzzz").is_empty());
    }
}
