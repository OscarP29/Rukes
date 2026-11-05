//! `http`: un mini Postman. `http 3000/api` hace `GET http://localhost:3000/api`;
//! `post :8080/login {"u":"ana"}` envía JSON; `-H "Nombre: valor"` añade cabeceras. La petición
//! solo sale al pulsar Enter (nunca al escribir) y la respuesta se muestra en el launcher:
//! estado, tiempo, tamaño, cabeceras y cuerpo (JSON formateado) en el panel de detalles.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, Icon, Plugin, PluginError, PluginMetadata, Query,
    ResultItem,
};
use serde::{Deserialize, Serialize};

/// Métodos que también sirven como palabra clave (`post :3000/x`).
const METHODS: [&str; 6] = ["get", "post", "put", "patch", "delete", "head"];
/// Palabras clave propias.
const KEYWORDS: [&str; 2] = ["http", "req"];
/// Peticiones recordadas (solo en memoria: pueden llevar tokens).
const HISTORY: usize = 30;
/// Tamaño máximo de la respuesta.
const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Texto como mucho en el panel de detalles (lo copiado es el cuerpo entero).
const MAX_DETAIL: usize = 16 * 1024;
/// Tiempo máximo de cada fase de la petición.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Una petición.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Req {
    /// Método en mayúsculas.
    pub method: String,
    /// URL completa.
    pub url: String,
    /// Cabeceras.
    pub headers: Vec<(String, String)>,
    /// Cuerpo.
    pub body: String,
}

impl Req {
    /// La petición como orden de `curl` (para copiarla).
    pub fn to_curl(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        let mut s = format!("curl -X {} {}", self.method, q(&self.url));
        for (k, v) in &self.headers {
            s.push_str(&format!(" -H {}", q(&format!("{k}: {v}"))));
        }
        if !self.body.is_empty() {
            s.push_str(&format!(" --data {}", q(&self.body)));
        }
        s
    }

    fn label(&self) -> String {
        format!("{} {}", self.method, self.url)
    }
}

/// Completa una dirección: un número (`3000`, `:3000/x`) es un puerto de localhost; `localhost`,
/// las IP y los nombres sin punto van por `http`; el resto por `https`.
pub fn normalize_url(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(s.to_owned());
    }
    if let Some(path) = s.strip_prefix('/') {
        return Some(format!("http://localhost/{path}"));
    }
    let rest = s.strip_prefix(':').unwrap_or(s);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = &rest[digits.len()..];
    if !digits.is_empty()
        && (after.is_empty() || after.starts_with(['/', '?', '#']))
        && digits.parse::<u16>().is_ok_and(|p| p > 0)
    {
        return Some(format!("http://localhost:{digits}{after}"));
    }
    let host = s
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit_once(':')
        .map_or(s.split(['/', '?', '#']).next().unwrap_or(""), |(h, _)| h);
    if host.is_empty() {
        return None;
    }
    let local = host.eq_ignore_ascii_case("localhost")
        || host.starts_with('[')
        || host.chars().all(|c| c.is_ascii_digit() || c == '.')
        || !host.contains('.');
    Some(format!("{}://{s}", if local { "http" } else { "https" }))
}

/// Siguiente palabra (con comillas simples o dobles) y lo que queda.
fn next_token(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let mut chars = s.char_indices();
    let (_, first) = chars.next()?;
    if first == '"' || first == '\'' {
        let end = s[1..].find(first)? + 1;
        return Some((s[1..end].to_owned(), &s[end + 1..]));
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    Some((s[..end].to_owned(), &s[end..]))
}

/// Interpreta `[MÉTODO] dirección [-H "Nombre: valor"]… [cuerpo]`. `method` es el método si el
/// usuario lo escribió como palabra clave (`post …`).
pub fn parse(method: Option<&str>, text: &str) -> Option<Req> {
    let mut rest = text.trim();
    let mut method = method.map(str::to_ascii_uppercase);
    let (mut target, mut after) = next_token(rest)?;
    if method.is_none() && METHODS.contains(&target.to_ascii_lowercase().as_str()) {
        method = Some(target.to_ascii_uppercase());
        (target, after) = next_token(after)?;
    }
    let url = normalize_url(&target)?;
    rest = after.trim_start();
    let mut headers = Vec::new();
    loop {
        let Some(h) = rest
            .strip_prefix("-H ")
            .or_else(|| rest.strip_prefix("--header "))
        else {
            break;
        };
        let (value, more) = next_token(h)?;
        if let Some((k, v)) = value.split_once(':') {
            headers.push((k.trim().to_owned(), v.trim().to_owned()));
        }
        rest = more.trim_start();
    }
    let mut body = rest
        .strip_prefix("-d ")
        .or_else(|| rest.strip_prefix("--data "))
        .unwrap_or(rest)
        .trim()
        .to_owned();
    if body.len() >= 2 && body.starts_with('\'') && body.ends_with('\'') {
        body = body[1..body.len() - 1].to_owned();
    }
    let method = method.unwrap_or_else(|| if body.is_empty() { "GET" } else { "POST" }.into());
    let has_type = headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
    if !body.is_empty() && !has_type {
        let json = body.starts_with('{') || body.starts_with('[');
        headers.push((
            "Content-Type".into(),
            if json {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            }
            .into(),
        ));
    }
    Some(Req {
        method,
        url,
        headers,
        body,
    })
}

/// `1234` → `1,2 KB`.
fn size(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0).replace('.', ",")
    } else {
        format!("{:.1} MB", n as f64 / 1_048_576.0).replace('.', ",")
    }
}

/// Cuerpo legible: JSON formateado, texto o un aviso si es binario.
fn body_text(body: &[u8], content_type: &str) -> String {
    if body.is_empty() {
        return tr("http.empty_body").to_owned();
    }
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body)
        && let Ok(pretty) = serde_json::to_string_pretty(&v)
    {
        return pretty;
    }
    let textual = content_type.is_empty()
        || content_type.starts_with("text/")
        || ["json", "xml", "javascript", "html", "csv", "yaml", "form"]
            .iter()
            .any(|t| content_type.contains(t));
    match std::str::from_utf8(body) {
        Ok(s) if textual || !s.contains('\0') => s.to_owned(),
        _ => trf("http.binary", &[("size", &size(body.len()))]),
    }
}

fn clip(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
        s.push_str("\n…");
    }
    s
}

#[derive(Debug, Clone)]
struct Entry {
    req: Req,
    status: Option<u32>,
    ms: u128,
    at: Instant,
}

/// Plugin `http`.
#[derive(Default)]
pub struct HttpPlugin {
    history: Mutex<VecDeque<Entry>>,
}

impl HttpPlugin {
    /// Plugin con el historial vacío.
    pub fn new() -> Self {
        Self::default()
    }

    fn remember(&self, req: &Req, status: Option<u32>, ms: u128) {
        let mut h = self.history.lock().unwrap_or_else(|p| p.into_inner());
        h.retain(|e| e.req != *req);
        h.push_front(Entry {
            req: req.clone(),
            status,
            ms,
            at: Instant::now(),
        });
        h.truncate(HISTORY);
    }

    fn request_item(req: &Req) -> ResultItem {
        let mut it = ResultItem::new("send", req.label());
        let mut parts = vec![tr("http.send").to_owned()];
        if !req.headers.is_empty() {
            parts.push(trf("http.headers_n", &[("n", &req.headers.len())]));
        }
        if !req.body.is_empty() {
            parts.push(trf("http.body_n", &[("size", &size(req.body.len()))]));
        }
        it.subtitle = parts.join(" · ");
        it.icon = Icon::Named("http".into());
        it.fuzzy = 1.0;
        it.payload = serde_json::to_string(req).unwrap_or_default();
        it.detail = Some(req.to_curl());
        it
    }

    fn history_items(&self) -> Vec<ResultItem> {
        let h = self.history.lock().unwrap_or_else(|p| p.into_inner());
        h.iter()
            .enumerate()
            .map(|(i, e)| {
                let mut it = Self::request_item(&e.req);
                it.id = format!("hist:{i}");
                let ago = e.at.elapsed().as_secs() / 60;
                let status = e.status.map_or_else(|| "—".to_owned(), |s| s.to_string());
                it.subtitle = trf(
                    "http.history_sub",
                    &[("status", &status), ("ms", &e.ms), ("min", &ago)],
                );
                it.fuzzy = 1.0 - i as f32 * 0.001;
                it
            })
            .collect()
    }

    /// Envía la petición y prepara los resultados de la respuesta.
    fn send(&self, req: &Req) -> Vec<ResultItem> {
        let t = Instant::now();
        let r = rukes_platform::http::send(&rukes_platform::http::Request {
            method: &req.method,
            url: &req.url,
            headers: &req.headers,
            body: req.body.as_bytes(),
            timeout: TIMEOUT,
            max_bytes: MAX_BYTES,
        });
        let ms = t.elapsed().as_millis();
        let payload = serde_json::to_string(req).unwrap_or_default();
        let resp = match r {
            Ok(resp) => resp,
            Err(e) => {
                self.remember(req, None, ms);
                let mut it = ResultItem::new("res:status", tr("http.failed"));
                it.subtitle = format!("{} · {e}", req.label());
                it.icon = Icon::Named("error".into());
                it.fuzzy = 1.0;
                it.payload = payload;
                it.detail = Some(format!("{}\n\n{e}\n\n{}", req.label(), tr("http.retry")));
                return vec![it];
            }
        };
        self.remember(req, Some(resp.status), ms);
        let content_type = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.to_ascii_lowercase())
            .unwrap_or_default();
        let full = body_text(&resp.body, &content_type);
        let body = clip(full.clone(), MAX_DETAIL);
        let ok = (200..400).contains(&resp.status);
        let mut sz = size(resp.body.len());
        if resp.truncated {
            sz.push('+');
        }
        let mut out = Vec::new();

        let mut status = ResultItem::new(
            "res:status",
            format!("{} {} · {ms} ms · {sz}", resp.status, resp.reason).replace("  ", " "),
        );
        status.subtitle = format!("{} · {}", req.label(), tr("http.resend"));
        status.icon = Icon::Named(if ok { "ok" } else { "error" }.into());
        status.fuzzy = 1.0;
        status.payload = payload;
        status.detail = Some(body.clone());
        status.expand = true;
        out.push(status);

        let first = body.lines().next().unwrap_or("").trim();
        let mut b = ResultItem::new(
            "res:body",
            if first.is_empty() {
                tr("http.empty_body").to_owned()
            } else {
                clip(first.to_owned(), 120)
            },
        );
        b.subtitle = format!(
            "{} · {} · {}",
            tr("http.body"),
            if content_type.is_empty() {
                "—"
            } else {
                &content_type
            },
            tr("http.copy")
        );
        b.icon = Icon::Named("note".into());
        b.fuzzy = 0.99;
        b.detail = Some(body);
        b.payload = full;
        out.push(b);

        let headers: String = resp
            .headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}\n"))
            .collect();
        let mut h = ResultItem::new(
            "res:headers",
            trf("http.headers_n", &[("n", &resp.headers.len())]),
        );
        h.subtitle = format!("{} · {}", content_type, tr("http.copy"));
        h.icon = Icon::Named("log".into());
        h.fuzzy = 0.98;
        h.detail = Some(headers);
        out.push(h);

        let mut c = ResultItem::new("res:curl", tr("http.copy_curl"));
        c.subtitle = req.to_curl();
        c.icon = Icon::Named("shell".into());
        c.fuzzy = 0.97;
        c.detail = Some(req.to_curl());
        out.push(c);

        if req.method == "GET" {
            let mut o = ResultItem::new("res:browser", tr("http.open_browser"));
            o.subtitle = req.url.clone();
            o.icon = Icon::Named("web".into());
            o.fuzzy = 0.96;
            o.payload = req.url.clone();
            out.push(o);
        }
        out
    }
}

impl Plugin for HttpPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "http".into(),
            name: tr("http.name").into(),
            prefix: None,
            keywords: KEYWORDS
                .iter()
                .chain(METHODS.iter())
                .map(|k| (*k).to_owned())
                .collect(),
            priority: 0.5,
            global: false,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let word = q
            .raw
            .split_whitespace()
            .next()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let method = METHODS.contains(&word.as_str()).then_some(word.as_str());
        if q.text.trim().is_empty() {
            // Con un método solo no se lista nada: «delete» puede ser otra cosa.
            if method.is_none() {
                sink(self.history_items());
            }
            return;
        }
        if let Some(req) = parse(method, &q.text) {
            sink(vec![Self::request_item(&req)]);
        }
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if action != ActionId::OPEN {
            return Err(PluginError::UnsupportedAction(action.into()));
        }
        let copy = |text: &str| -> Result<ActionOutcome, PluginError> {
            rukes_platform::clipboard::set_text(text)
                .map_err(|e| PluginError::Failed(e.to_string()))?;
            Ok(ActionOutcome::Notify(tr("http.copied").to_owned()))
        };
        match item.id.as_str() {
            "res:body" => copy(&item.payload),
            "res:headers" | "res:curl" => copy(item.detail.as_deref().unwrap_or("")),
            "res:browser" => {
                rukes_platform::shell::launch(
                    &item.payload,
                    None,
                    None,
                    rukes_platform::shell::Verb::Open,
                )
                .map_err(|e| PluginError::Failed(e.to_string()))?;
                Ok(ActionOutcome::Hide)
            }
            _ => {
                let req: Req = serde_json::from_str(&item.payload)
                    .map_err(|e| PluginError::Failed(e.to_string()))?;
                Ok(ActionOutcome::ShowResults(self.send(&req)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_localhost_ports() {
        let n = |s| normalize_url(s).unwrap();
        assert_eq!(n("3000"), "http://localhost:3000");
        assert_eq!(
            n("3000/api/users?x=1"),
            "http://localhost:3000/api/users?x=1"
        );
        assert_eq!(n(":8080/login"), "http://localhost:8080/login");
        assert_eq!(n("/health"), "http://localhost/health");
        assert_eq!(n("localhost:5173"), "http://localhost:5173");
        assert_eq!(n("127.0.0.1:9000/x"), "http://127.0.0.1:9000/x");
        assert_eq!(n("miservidor:81"), "http://miservidor:81");
        assert_eq!(n("api.github.com/users"), "https://api.github.com/users");
        assert_eq!(n("HTTP://a.b"), "HTTP://a.b");
        assert!(
            !n("99999").contains("localhost"),
            "puerto no válido: se trata como host"
        );
        assert!(normalize_url("  ").is_none());
    }

    #[test]
    fn parses_method_headers_and_body() {
        let r = parse(None, "3000/api").unwrap();
        assert_eq!(
            (r.method.as_str(), r.url.as_str()),
            ("GET", "http://localhost:3000/api")
        );
        assert!(r.headers.is_empty() && r.body.is_empty());

        let r = parse(
            None,
            r#"put :8080/u/1 -H "Authorization: Bearer x" -H 'X-A: 1' {"name": "Ana"}"#,
        )
        .unwrap();
        assert_eq!(r.method, "PUT");
        assert_eq!(r.body, r#"{"name": "Ana"}"#);
        assert_eq!(
            r.headers,
            vec![
                ("Authorization".into(), "Bearer x".into()),
                ("X-A".into(), "1".into()),
                ("Content-Type".into(), "application/json".into()),
            ]
        );

        // Método como palabra clave; cuerpo sin método explícito → POST.
        let r = parse(Some("delete"), "3000/u/1").unwrap();
        assert_eq!(r.method, "DELETE");
        let r = parse(None, "3000/echo -d 'hola mundo'").unwrap();
        assert_eq!((r.method.as_str(), r.body.as_str()), ("POST", "hola mundo"));
        assert_eq!(r.headers[0].1, "text/plain; charset=utf-8");
        assert!(parse(None, "").is_none());
    }

    #[test]
    fn curl_and_sizes() {
        let r = parse(None, r#"post 3000/x {"a":"b"}"#).unwrap();
        assert_eq!(
            r.to_curl(),
            r#"curl -X POST "http://localhost:3000/x" -H "Content-Type: application/json" --data "{\"a\":\"b\"}""#
        );
        assert_eq!(size(10), "10 B");
        assert_eq!(size(1536), "1,5 KB");
        assert_eq!(body_text(br#"{"a":1}"#, ""), "{\n  \"a\": 1\n}");
        assert!(body_text(&[0, 159, 146, 150], "image/png").contains("B"));
    }

    #[test]
    fn sends_to_a_local_server_and_shows_the_response() {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            // Lee la petición entera (cabeceras y cuerpo pueden llegar por separado): cerrar
            // con datos sin leer cortaría la conexión.
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            while !String::from_utf8_lossy(&buf).ends_with("{\"n\":1}") {
                let n = s.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let n = buf.len();
            let body = br#"{"ok":true,"items":[1,2]}"#;
            let head = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            s.write_all(head.as_bytes()).unwrap();
            s.write_all(body).unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        let p = HttpPlugin::new();
        let req = parse(Some("post"), &format!("{port}/items {{\"n\":1}}")).unwrap();
        let items = p.send(&req);
        let seen = server.join().unwrap();
        assert!(seen.starts_with("POST /items HTTP/1.1"), "{seen}");
        assert!(seen.contains("Content-Type: application/json"));
        assert!(
            items[0].title.starts_with("201 Created · "),
            "{}",
            items[0].title
        );
        assert!(items[0].expand);
        assert!(items[0].detail.as_deref().unwrap().contains("\"items\": ["));
        assert_eq!(items[2].title, trf("http.headers_n", &[("n", &3)]));
        assert!(
            !items.iter().any(|i| i.id == "res:browser"),
            "solo para GET"
        );
        // Queda en el historial y se puede repetir.
        let hist = p.history_items();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].title, format!("POST http://localhost:{port}/items"));
    }

    #[test]
    fn connection_errors_become_a_retry_item() {
        let p = HttpPlugin::new();
        let items = p.send(&parse(None, "http://127.0.0.1:9/").unwrap());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "res:status");
        assert!(!items[0].payload.is_empty());
    }
}
