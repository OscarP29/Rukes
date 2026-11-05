//! `http`: un mini Postman. `http` abre su ventana (método, dirección y pestañas de cabeceras,
//! cuerpo, autenticación y parámetros); `http 3000/api` hace `GET http://localhost:3000/api`;
//! `post :8080/login {"u":"ana"}` envía JSON; `-H "Nombre: valor"` añade cabeceras; una orden
//! `curl` pegada se convierte en la petición. Nada se envía hasta pulsar Enter.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use rukes_core::i18n::{tr, trf};
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, HttpForm, HttpResponse, Icon, Plugin, PluginError,
    PluginMetadata, Query, ResultItem,
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
/// Texto como mucho en la vista de la respuesta.
const MAX_SHOWN: usize = 512 * 1024;
/// Tiempo máximo de cada fase de la petición.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Una petición lista para enviar.
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

    /// La petición como formulario editable.
    pub fn to_form(&self) -> HttpForm {
        HttpForm {
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self
                .headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join("\n"),
            body: self.body.clone(),
            auth: String::new(),
            params: String::new(),
        }
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
    if host.is_empty() || host.contains(char::is_whitespace) {
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

/// Pone `Content-Type` si hay cuerpo y no lo tiene: JSON si lo parece, si no texto.
fn add_content_type(headers: &mut Vec<(String, String)>, body: &str) {
    let has_type = headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
    if !body.is_empty() && !has_type {
        let b = body.trim_start();
        let json = (b.starts_with('{') || b.starts_with('['))
            && serde_json::from_str::<serde_json::Value>(body).is_ok();
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
    add_content_type(&mut headers, &body);
    Some(Req {
        method,
        url,
        headers,
        body,
    })
}

// ------------------------------------------------------------------------------------ curl

/// Parte una orden de shell en palabras: comillas simples y dobles, `\` (bash) y `^` (cmd de
/// Windows, como la copia «Copy as cURL (cmd)» de los navegadores), `$'…'` y líneas partidas.
pub fn shell_words(text: &str) -> Vec<String> {
    // cmd: `^` escapa el siguiente carácter y `^` al final de la línea la continúa.
    let cmd = text.contains("^\"") || text.contains("^\n") || text.contains("^\r\n");
    let text = if cmd {
        let joined = text.replace("^\r\n", " ").replace("^\n", " ");
        let mut out = String::with_capacity(joined.len());
        let mut chars = joined.chars();
        while let Some(c) = chars.next() {
            if c == '^' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        text.replace("\\\r\n", " ").replace("\\\n", " ")
    };
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    cur.push(c);
                }
            }
            '$' if chars.peek() == Some(&'\'') => {
                chars.next();
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '\'' => break,
                        '\\' => match chars.next() {
                            Some('n') => cur.push('\n'),
                            Some('t') => cur.push('\t'),
                            Some('r') => cur.push('\r'),
                            Some(o) => cur.push(o),
                            None => {}
                        },
                        o => cur.push(o),
                    }
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            if let Some(n) = chars.next() {
                                cur.push(n);
                            }
                        }
                        o => cur.push(o),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            o => {
                in_word = true;
                cur.push(o);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    words
}

/// Opciones de curl que llevan un valor que aquí no se usa (se salta).
const CURL_SKIP_VALUE: &[&str] = &[
    "-o",
    "--output",
    "-m",
    "--max-time",
    "--connect-timeout",
    "-w",
    "--write-out",
    "-x",
    "--proxy",
    "-U",
    "--proxy-user",
    "--retry",
    "-c",
    "--cookie-jar",
    "--cacert",
    "--cert",
    "-E",
    "--key",
    "-r",
    "--range",
    "-T",
    "--upload-file",
    "-K",
    "--config",
    "--resolve",
    "--limit-rate",
    "--max-redirs",
    "-F",
    "--form",
];

/// Percent-encoding de un valor de query o formulario.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Interpreta una orden `curl` pegada: método, dirección, cabeceras, cuerpo y usuario.
pub fn parse_curl(text: &str) -> Option<HttpForm> {
    let words = shell_words(text.trim());
    let mut it = words.into_iter().peekable();
    let first = it.next()?;
    let exe = first.to_ascii_lowercase();
    if exe != "curl" && exe != "curl.exe" {
        return None;
    }
    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<String> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut auth = String::new();
    let mut get = false;
    let mut head = false;
    let mut json = false;
    while let Some(w) = it.next() {
        // `--opcion=valor` y `-Xvalor`.
        let (flag, attached) =
            if let Some((f, v)) = w.split_once('=').filter(|_| w.starts_with("--")) {
                (f.to_owned(), Some(v.to_owned()))
            } else if w.len() > 2
                && !w.starts_with("--")
                && w.get(..2)
                    .is_some_and(|p| ["-X", "-H", "-d", "-u", "-A", "-b", "-e"].contains(&p))
            {
                (w[..2].to_owned(), Some(w[2..].to_owned()))
            } else {
                (w.clone(), None)
            };
        let value = |it: &mut std::iter::Peekable<std::vec::IntoIter<String>>| {
            attached.clone().or_else(|| it.next()).unwrap_or_default()
        };
        match flag.as_str() {
            "-X" | "--request" => method = Some(value(&mut it).to_ascii_uppercase()),
            "-H" | "--header" => headers.push(value(&mut it)),
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => {
                data.push(value(&mut it));
            }
            "--data-urlencode" => {
                let v = value(&mut it);
                data.push(match v.split_once('=') {
                    Some((k, v)) => format!("{k}={}", encode(v)),
                    None => encode(&v),
                });
            }
            "--json" => {
                data.push(value(&mut it));
                json = true;
            }
            "-u" | "--user" => auth = value(&mut it),
            "-A" | "--user-agent" => headers.push(format!("User-Agent: {}", value(&mut it))),
            "-b" | "--cookie" => headers.push(format!("Cookie: {}", value(&mut it))),
            "-e" | "--referer" => headers.push(format!("Referer: {}", value(&mut it))),
            "--url" => url = Some(value(&mut it)),
            "-G" | "--get" => get = true,
            "-I" | "--head" => head = true,
            f if CURL_SKIP_VALUE.contains(&f) => {
                if attached.is_none() {
                    it.next();
                }
            }
            f if f.starts_with('-') && f.len() > 1 => {}
            _ => {
                if url.is_none() {
                    url = Some(w);
                }
            }
        }
    }
    fn has(headers: &[String], name: &str) -> bool {
        headers.iter().any(|h| {
            h.split(':')
                .next()
                .is_some_and(|k| k.trim().eq_ignore_ascii_case(name))
        })
    }
    if json {
        if !has(&headers, "content-type") {
            headers.push("Content-Type: application/json".into());
        }
        if !has(&headers, "accept") {
            headers.push("Accept: application/json".into());
        }
    }
    let mut url = url?;
    let joined = data.join("&");
    // Con -G los datos (ya codificados) van a la query.
    let body = if get {
        if !joined.is_empty() {
            let sep = if url.contains('?') { '&' } else { '?' };
            url = format!("{url}{sep}{joined}");
        }
        String::new()
    } else {
        joined
    };
    if !body.is_empty() && !has(&headers, "content-type") {
        // curl -d manda formularios por defecto.
        let looks_json = serde_json::from_str::<serde_json::Value>(&body)
            .is_ok_and(|v| v.is_object() || v.is_array());
        headers.push(
            if looks_json {
                "Content-Type: application/json"
            } else {
                "Content-Type: application/x-www-form-urlencoded"
            }
            .into(),
        );
    }
    let method = method.unwrap_or_else(|| {
        if head {
            "HEAD"
        } else if body.is_empty() {
            "GET"
        } else {
            "POST"
        }
        .into()
    });
    Some(HttpForm {
        method,
        url,
        headers: headers.join("\n"),
        body,
        auth,
        params: String::new(),
    })
}

/// Formulario a partir de lo que se escribe o pega en la barra: una orden `curl` o la sintaxis
/// corta (`post 3000/x {"a":1}`).
pub fn form_from_text(text: &str) -> Option<HttpForm> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("curl ") || lower.starts_with("curl.exe ") {
        return parse_curl(t);
    }
    parse(None, t).map(|r| r.to_form())
}

/// Cabecera `Authorization` a partir de la pestaña Auth.
fn auth_header(auth: &str) -> Option<String> {
    let a = auth.trim();
    if a.is_empty() {
        return None;
    }
    let b64 = |s: &str| base64::engine::general_purpose::STANDARD.encode(s.as_bytes());
    if let Some(rest) = a
        .strip_prefix("Basic ")
        .or_else(|| a.strip_prefix("basic "))
    {
        let rest = rest.trim();
        return Some(if rest.contains(':') {
            format!("Basic {}", b64(rest))
        } else {
            format!("Basic {rest}")
        });
    }
    if a.contains(char::is_whitespace) {
        // `Bearer x`, `Token x`…
        return Some(a.to_owned());
    }
    Some(if a.contains(':') {
        format!("Basic {}", b64(a))
    } else {
        format!("Bearer {a}")
    })
}

/// Construye la petición a partir del formulario: completa la dirección, añade los parámetros,
/// la autenticación y el `Content-Type`.
pub fn build(form: &HttpForm) -> Result<Req, String> {
    let mut url = normalize_url(&form.url).ok_or_else(|| tr("http.bad_url").to_owned())?;
    let params: Vec<String> = form
        .params
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.split_once('=') {
            Some((k, v)) => format!("{}={}", encode(k.trim()), encode(v.trim())),
            None => encode(l),
        })
        .collect();
    if !params.is_empty() {
        let (base, frag) = url
            .split_once('#')
            .map_or((url.as_str(), None), |(b, f)| (b, Some(f)));
        let sep = if base.contains('?') { '&' } else { '?' };
        let mut u = format!("{base}{sep}{}", params.join("&"));
        if let Some(f) = frag {
            u.push('#');
            u.push_str(f);
        }
        url = u;
    }
    let mut headers = Vec::new();
    for line in form.headers.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| trf("http.bad_header", &[("line", &line)]))?;
        headers.push((k.trim().to_owned(), v.trim().to_owned()));
    }
    if let Some(a) = auth_header(&form.auth)
        && !headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
    {
        headers.push(("Authorization".into(), a));
    }
    let body = if form.body.trim().is_empty() {
        String::new()
    } else {
        form.body.clone()
    };
    let method = match form.method.trim() {
        "" if body.is_empty() => "GET".to_owned(),
        "" => "POST".to_owned(),
        m => m.to_ascii_uppercase(),
    };
    add_content_type(&mut headers, &body);
    Ok(Req {
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
    form: HttpForm,
    label: String,
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

    fn remember(&self, form: &HttpForm, req: &Req, status: Option<u32>, ms: u128) {
        let mut h = self.history.lock().unwrap_or_else(|p| p.into_inner());
        h.retain(|e| e.form != *form);
        h.push_front(Entry {
            form: form.clone(),
            label: req.label(),
            status,
            ms,
            at: Instant::now(),
        });
        h.truncate(HISTORY);
    }

    /// Peticiones recientes, la última primero.
    pub fn history(&self) -> Vec<HttpForm> {
        self.history
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|e| e.form.clone())
            .collect()
    }

    /// Envía la petición del formulario (bloqueante) y la guarda en el historial.
    pub fn send(&self, form: &HttpForm) -> Result<HttpResponse, String> {
        let req = build(form)?;
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
        let resp = match r {
            Ok(resp) => resp,
            Err(e) => {
                self.remember(form, &req, None, ms);
                return Err(format!("{}: {e}", tr("http.failed")));
            }
        };
        self.remember(form, &req, Some(resp.status), ms);
        let content_type = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.to_ascii_lowercase())
            .unwrap_or_default();
        Ok(HttpResponse {
            status: resp.status,
            reason: resp.reason.trim().to_owned(),
            ms,
            size: resp.body.len(),
            truncated: resp.truncated,
            headers: resp
                .headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join("\n"),
            body: clip(body_text(&resp.body, &content_type), MAX_SHOWN),
            content_type,
            curl: req.to_curl(),
        })
    }

    fn open_item(form: HttpForm, send: bool) -> ActionOutcome {
        ActionOutcome::OpenHttp {
            form: Box::new(form),
            send,
        }
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
        it.payload = serde_json::to_string(&req.to_form()).unwrap_or_default();
        it.detail = Some(req.to_curl());
        it
    }

    fn list_items(&self) -> Vec<ResultItem> {
        let mut open = ResultItem::new("open", tr("http.open"));
        open.subtitle = tr("http.open_sub").into();
        open.icon = Icon::Named("http".into());
        open.fuzzy = 1.0;
        open.payload = serde_json::to_string(&HttpForm::default()).unwrap_or_default();
        let mut out = vec![open];
        let h = self.history.lock().unwrap_or_else(|p| p.into_inner());
        for (i, e) in h.iter().enumerate() {
            let mut it = ResultItem::new(format!("hist:{i}"), e.label.clone());
            let ago = e.at.elapsed().as_secs() / 60;
            let status = e.status.map_or_else(|| "—".to_owned(), |s| s.to_string());
            it.subtitle = trf(
                "http.history_sub",
                &[("status", &status), ("ms", &e.ms), ("min", &ago)],
            );
            it.icon = Icon::Named("http".into());
            it.fuzzy = 0.99 - i as f32 * 0.001;
            it.payload = serde_json::to_string(&e.form).unwrap_or_default();
            out.push(it);
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
                .chain(["curl"].iter())
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
        if word == "curl" {
            if let Some(form) = parse_curl(q.raw.trim())
                && let Ok(req) = build(&form)
            {
                let mut it = Self::request_item(&req);
                it.payload = serde_json::to_string(&form).unwrap_or_default();
                sink(vec![it]);
            }
            return;
        }
        let method = METHODS.contains(&word.as_str()).then_some(word.as_str());
        if q.text.trim().is_empty() {
            // Con un método solo no se lista nada: «delete» puede ser otra cosa.
            if method.is_none() {
                sink(self.list_items());
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
        let form: HttpForm =
            serde_json::from_str(&item.payload).map_err(|e| PluginError::Failed(e.to_string()))?;
        // Lo escrito se envía; el historial y «abrir» solo cargan la petición (puede ser un
        // DELETE o un POST que no conviene repetir sin mirar).
        Ok(Self::open_item(form, item.id == "send"))
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
        assert!(normalize_url("dos palabras").is_none());
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
    fn pasted_curl_fills_everything() {
        // bash, como lo copian los navegadores.
        let f = parse_curl(
            "curl 'https://api.x.com/v1/users?page=2' \\\n  -X 'PUT' \\\n  -H 'accept: application/json' \\\n  -H 'authorization: Bearer abc' \\\n  --data-raw '{\"name\":\"Ana López\"}' \\\n  --compressed",
        )
        .unwrap();
        assert_eq!(f.method, "PUT");
        assert_eq!(f.url, "https://api.x.com/v1/users?page=2");
        assert_eq!(
            f.headers,
            "accept: application/json\nauthorization: Bearer abc\nContent-Type: application/json"
        );
        assert_eq!(f.body, r#"{"name":"Ana López"}"#);

        // cmd de Windows («Copy as cURL (cmd)»).
        let f = parse_curl(
            "curl ^\"https://x.com/api^\" ^\n  -H ^\"content-type: application/json^\" ^\n  --data-raw ^\"^{^\\^\"a^\\^\":1^}^\"",
        )
        .unwrap();
        assert_eq!(
            (f.method.as_str(), f.url.as_str()),
            ("POST", "https://x.com/api")
        );
        assert_eq!(f.body, r#"{"a":1}"#);
        assert_eq!(f.headers, "content-type: application/json");

        // Formulario, usuario, -G, opciones pegadas y desconocidas.
        let f = parse_curl(
            r#"curl -sSL -u ana:secreta -G --data-urlencode "q=hola mundo" -d n=1 -m 10 -XGET "http://localhost:3000/buscar""#,
        )
        .unwrap();
        assert_eq!(f.method, "GET");
        assert_eq!(f.url, "http://localhost:3000/buscar?q=hola%20mundo&n=1");
        assert_eq!(f.auth, "ana:secreta");
        assert!(f.body.is_empty() && f.params.is_empty());
        let f = parse_curl("curl -d a=1 -d b=2 --url=3000/x").unwrap();
        assert_eq!((f.method.as_str(), f.body.as_str()), ("POST", "a=1&b=2"));
        assert!(f.headers.contains("x-www-form-urlencoded"));
        let f = parse_curl(r#"curl --json '{"a":1}' 3000"#).unwrap();
        assert!(f.headers.contains("Accept: application/json"));
        assert_eq!(parse_curl("curl -I example.com").unwrap().method, "HEAD");
        assert!(parse_curl("wget x").is_none());
        assert!(parse_curl("curl -s").is_none());
        // La barra acepta curl y la sintaxis corta.
        assert_eq!(form_from_text("curl 3000").unwrap().url, "3000");
        let f = form_from_text("post 3000/x {\"a\":1}").unwrap();
        assert_eq!(f.method, "POST");
        assert_eq!(f.url, "http://localhost:3000/x");
    }

    #[test]
    fn form_builds_the_request() {
        let form = HttpForm {
            method: String::new(),
            url: "3000/u?x=1#top".into(),
            headers: "X-A: 1\n\n# comentario\n  Accept : text/html  ".into(),
            body: "{\"a\": 1}".into(),
            auth: "ana:secreta".into(),
            params: "q=hola mundo\nvacío".into(),
        };
        let r = build(&form).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(
            r.url,
            "http://localhost:3000/u?x=1&q=hola%20mundo&vac%C3%ADo#top"
        );
        assert_eq!(
            r.headers,
            vec![
                ("X-A".into(), "1".into()),
                ("Accept".into(), "text/html".into()),
                ("Authorization".into(), "Basic YW5hOnNlY3JldGE=".into()),
                ("Content-Type".into(), "application/json".into()),
            ]
        );
        let auth = |a: &str| auth_header(a);
        assert_eq!(auth("tok123").unwrap(), "Bearer tok123");
        assert_eq!(auth("Bearer x").unwrap(), "Bearer x");
        assert_eq!(auth("Basic YQ==").unwrap(), "Basic YQ==");
        assert_eq!(auth("basic a:b").unwrap(), "Basic YTpi");
        assert!(auth("  ").is_none());
        // Una cabecera Authorization escrita gana a la pestaña Auth.
        let r = build(&HttpForm {
            url: "3000".into(),
            headers: "Authorization: Token y".into(),
            auth: "x".into(),
            method: "get".into(),
            ..HttpForm::default()
        })
        .unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.headers, vec![("Authorization".into(), "Token y".into())]);
        assert!(build(&HttpForm::default()).is_err());
        assert!(
            build(&HttpForm {
                url: "3000".into(),
                headers: "sin dos puntos".into(),
                ..HttpForm::default()
            })
            .is_err()
        );
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
        // Lo escrito con curl vuelve a la misma petición.
        let f = parse_curl(&r.to_curl()).unwrap();
        assert_eq!(build(&f).unwrap(), r);
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
        let form = HttpForm {
            method: "POST".into(),
            url: format!("{port}/items"),
            body: "{\"n\":1}".into(),
            auth: "tok".into(),
            ..HttpForm::default()
        };
        let r = p.send(&form).unwrap();
        let seen = server.join().unwrap();
        assert!(seen.starts_with("POST /items HTTP/1.1"), "{seen}");
        assert!(seen.contains("Content-Type: application/json"));
        assert!(seen.contains("Authorization: Bearer tok"));
        assert_eq!((r.status, r.reason.as_str()), (201, "Created"));
        assert!(r.body.contains("\"items\": ["), "{}", r.body);
        assert_eq!(r.size, 25);
        assert_eq!(r.content_type, "application/json");
        assert!(r.headers.contains("Content-Length: 25"));
        assert!(r.curl.starts_with("curl -X POST"));
        // Queda en el historial.
        assert_eq!(p.history(), vec![form]);
    }

    #[test]
    fn launcher_items_open_the_window() {
        let _ = rukes_core::i18n::set_language("es", "");
        let p = HttpPlugin::new();
        let q = |raw: &str, text: &str| Query {
            id: rukes_core::QueryId(1),
            raw: raw.into(),
            text: text.into(),
            targeted: true,
        };
        let mut got = Vec::new();
        p.query(&q("http ", ""), &CancelToken::default(), &mut |v| got = v);
        assert_eq!(got[0].id, "open");
        assert!(matches!(
            p.execute(&got[0], ActionId::OPEN).unwrap(),
            ActionOutcome::OpenHttp { send: false, .. }
        ));
        p.query(
            &q("post 3000/x {}", "3000/x {}"),
            &CancelToken::default(),
            &mut |v| got = v,
        );
        match p.execute(&got[0], ActionId::OPEN).unwrap() {
            ActionOutcome::OpenHttp { form, send } => {
                assert!(send);
                assert_eq!(form.method, "POST");
                assert_eq!(form.url, "http://localhost:3000/x");
            }
            other => panic!("{other:?}"),
        }
        p.query(
            &q("curl -X DELETE 3000/u/1", "-X DELETE 3000/u/1"),
            &CancelToken::default(),
            &mut |v| got = v,
        );
        assert_eq!(got[0].title, "DELETE http://localhost:3000/u/1");
    }

    #[test]
    fn connection_errors_are_reported_and_remembered() {
        let p = HttpPlugin::new();
        let form = HttpForm {
            url: "http://127.0.0.1:9/".into(),
            ..HttpForm::default()
        };
        assert!(p.send(&form).is_err());
        assert_eq!(p.history().len(), 1);
    }
}
