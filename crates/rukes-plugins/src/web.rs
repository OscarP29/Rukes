//! URLs escritas directamente y búsquedas web con palabra clave (RF-090, RF-091).

use std::collections::BTreeMap;

use rukes_core::i18n::{Lang, language};
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, Icon, Plugin, PluginError, PluginMetadata, Query,
    ResultItem,
};
use rukes_platform::shell::{self, Verb};

use crate::util::{self, percent_encode};

/// Plugin web.
pub struct WebPlugin {
    /// Palabra clave → plantilla de URL con `{q}`.
    pub engines: BTreeMap<String, String>,
}

/// Extensiones que no son dominios (para no tomar `notas.txt` por una URL).
const FILE_EXTENSIONS: &[&str] = &[
    "exe", "msi", "lnk", "dll", "sys", "bat", "cmd", "ps1", "txt", "md", "pdf", "doc", "docx",
    "xls", "xlsx", "ppt", "pptx", "csv", "json", "toml", "yaml", "yml", "xml", "ini", "log", "png",
    "jpg", "jpeg", "gif", "bmp", "webp", "svg", "mp3", "mp4", "mkv", "avi", "wav", "zip", "rar",
    "7z", "tar", "gz", "rs", "py", "js", "ts", "c", "h", "cpp", "cs", "java", "go", "html", "css",
];

/// Si `s` parece una URL, la devuelve completa (con esquema).
pub fn as_url(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if ["http://", "https://", "ftp://", "file://"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return (s.len() > lower.find("://")? + 3).then(|| s.to_owned());
    }
    let host_port = s.split(['/', '?', '#']).next()?;
    let host = host_port.split(':').next()?;
    if let Some(port) = host_port.split(':').nth(1)
        && (port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    if host.eq_ignore_ascii_case("localhost") {
        return Some(format!("http://{s}"));
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2
        || labels
            .iter()
            .any(|l| l.is_empty() || !l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    {
        return None;
    }
    // IPv4.
    if labels.len() == 4 && labels.iter().all(|l| l.parse::<u8>().is_ok()) {
        return Some(format!("http://{s}"));
    }
    let tld = labels.last()?.to_ascii_lowercase();
    let tld_ok = (2..=12).contains(&tld.len())
        && tld.chars().all(|c| c.is_ascii_alphabetic())
        && (host_port.len() < s.len() || !FILE_EXTENSIONS.contains(&tld.as_str()));
    tld_ok.then(|| format!("https://{s}"))
}

/// Nombre legible de un buscador a partir de su URL (`www.google.com` → `google.com`).
fn engine_name(template: &str) -> String {
    let host = template
        .split("://")
        .nth(1)
        .unwrap_or(template)
        .split('/')
        .next()
        .unwrap_or("");
    host.trim_start_matches("www.").to_owned()
}

fn link(id: &str, title: String, subtitle: String, url: String) -> ResultItem {
    let mut it = ResultItem::new(id, title);
    it.subtitle = subtitle;
    it.icon = Icon::Named("web".into());
    it.fuzzy = 1.0;
    it.actions.push(rukes_core::Action::new(
        ActionId::COPY_PATH,
        "action.copy_path",
    ));
    it.payload = url;
    it
}

impl Plugin for WebPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "web".into(),
            name: "Web".into(),
            prefix: None,
            keywords: self.engines.keys().cloned().collect(),
            priority: 0.3,
            global: true,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let mut out = Vec::new();
        if q.targeted {
            // Dirigida por palabra clave: la primera palabra de la consulta original.
            let keyword = q
                .raw
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if let Some(template) = self.engines.get(&keyword)
                && !q.text.is_empty()
            {
                let url = template.replace("{q}", &percent_encode(&q.text));
                let name = engine_name(template);
                let title = match language() {
                    Lang::Es => format!("Buscar «{}» en {name}", q.text),
                    Lang::En => format!("Search {name} for “{}”", q.text),
                };
                out.push(link(&format!("search:{keyword}"), title, url.clone(), url));
            }
        } else if let Some(url) = as_url(&q.text) {
            let title = match language() {
                Lang::Es => format!("Abrir {}", q.text),
                Lang::En => format!("Open {}", q.text),
            };
            out.push(link("url", title, url.clone(), url));
        }
        sink(out);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        match action {
            ActionId::OPEN => shell::launch(&item.payload, None, None, Verb::Open)
                .map(|()| ActionOutcome::Hide)
                .map_err(|e| PluginError::Failed(e.to_string())),
            ActionId::COPY_PATH => util::copy(&item.payload),
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_urls() {
        assert_eq!(as_url("github.com").as_deref(), Some("https://github.com"));
        assert_eq!(
            as_url("docs.rs/iced/latest").as_deref(),
            Some("https://docs.rs/iced/latest")
        );
        assert_eq!(
            as_url("https://x.y/z?a=1").as_deref(),
            Some("https://x.y/z?a=1")
        );
        assert_eq!(
            as_url("localhost:8080").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(as_url("192.168.1.1").as_deref(), Some("http://192.168.1.1"));
        assert_eq!(as_url("notas.txt"), None);
        assert_eq!(as_url("setup.exe"), None);
        assert_eq!(as_url("google chrome"), None);
        assert_eq!(as_url("3.14"), None);
        assert_eq!(as_url("https://"), None);
        assert_eq!(as_url("a.b:xx"), None);
    }

    #[test]
    fn keyword_search() {
        let mut engines = BTreeMap::new();
        engines.insert(
            "g".to_owned(),
            "https://www.google.com/search?q={q}".to_owned(),
        );
        let p = WebPlugin { engines };
        let q = Query {
            id: rukes_core::QueryId(1),
            raw: "g rust lifetimes".into(),
            text: "rust lifetimes".into(),
            targeted: true,
        };
        let mut got = Vec::new();
        p.query(&q, &CancelToken::new(), &mut |b| got.extend(b));
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].payload,
            "https://www.google.com/search?q=rust%20lifetimes"
        );
        assert!(got[0].title.contains("google.com"));
    }
}
