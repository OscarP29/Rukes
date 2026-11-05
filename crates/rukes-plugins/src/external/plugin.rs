//! Plugins externos en el motor (RF-170..RF-173) y la lista `plugins`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action as UiAction, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError,
    PluginMetadata, Query, ResultItem,
};
use serde_json::{Value, json};

use super::manifest::{Manifest, discover};
use super::process::{CallError, PluginProcess};

/// Tiempo máximo de `query`.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
/// Tiempo máximo de `action`.
pub const ACTION_TIMEOUT: Duration = Duration::from_secs(10);

/// Un plugin externo.
pub struct ExternalPlugin {
    /// Manifiesto.
    pub manifest: Manifest,
    /// Su proceso.
    pub process: PluginProcess,
}

fn resolve_path(dir: &Path, s: &str) -> PathBuf {
    let p = PathBuf::from(s);
    if p.is_absolute() { p } else { dir.join(p) }
}

fn icon_for(dir: &Path, s: Option<&str>) -> Icon {
    match s {
        Some(s) if s.contains('.') => Icon::File(resolve_path(dir, s)),
        Some(s) if !s.trim().is_empty() => Icon::Named(s.trim().to_owned()),
        _ => Icon::Named("plugin".into()),
    }
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

impl ExternalPlugin {
    /// Prepara el plugin (el proceso arranca en la primera consulta).
    pub fn new(manifest: Manifest) -> Self {
        let process = PluginProcess::new(
            &manifest.name,
            &manifest.dir,
            &manifest.executable,
            &manifest.args,
            manifest.config.clone(),
        );
        Self { manifest, process }
    }

    /// Convierte la lista de resultados del protocolo.
    pub fn items(&self, list: &Value) -> Vec<ResultItem> {
        let list = list.get("items").unwrap_or(list);
        let Some(arr) = list.as_array() else {
            return Vec::new();
        };
        let dir = &self.manifest.dir;
        arr.iter()
            .enumerate()
            .filter_map(|(i, v)| {
                let title = str_of(v, "title")?;
                let id = match v.get("id") {
                    Some(Value::String(s)) => s.clone(),
                    Some(Value::Number(n)) => n.to_string(),
                    _ => i.to_string(),
                };
                let mut it = ResultItem::new(format!("i:{id}"), title);
                it.subtitle = str_of(v, "subtitle").unwrap_or_default().to_owned();
                it.icon = icon_for(dir, str_of(v, "icon").or(self.manifest.icon.as_deref()));
                it.fuzzy = v
                    .get("score")
                    .and_then(Value::as_f64)
                    .map_or(1.0 - i as f32 * 0.001, |s| s.clamp(0.0, 1.0) as f32);
                it.preview = str_of(v, "preview").map(|p| resolve_path(dir, p));
                it.detail = str_of(v, "detail").map(str::to_owned);
                if let Some(actions) = v.get("actions").and_then(Value::as_array) {
                    let list: Vec<UiAction> = actions
                        .iter()
                        .filter_map(|a| match a {
                            Value::String(s) => Some(UiAction::new(s, action_label(s))),
                            Value::Object(_) => {
                                let id = str_of(a, "id")?;
                                let label = str_of(a, "label").unwrap_or(id);
                                Some(UiAction::new(id, label))
                            }
                            _ => None,
                        })
                        .collect();
                    if !list.is_empty() {
                        it.actions = list;
                    }
                }
                it.payload =
                    json!({ "id": id, "data": v.get("data").cloned().unwrap_or(Value::Null) })
                        .to_string();
                Some(it)
            })
            .collect()
    }

    fn error_item(&self, err: &CallError) -> ResultItem {
        let title = if matches!(err, CallError::Disabled) {
            trf("external.disabled", &[("name", &self.manifest.name)])
        } else {
            trf("external.failed", &[("name", &self.manifest.name)])
        };
        let mut it = ResultItem::new("err", title);
        let detail = self.process.last_error().unwrap_or_else(|| err.to_string());
        it.subtitle = format!("{detail} · {}", tr("external.restart_hint"));
        it.icon = Icon::Named("plugin".into());
        it.fuzzy = 1.0;
        it.actions = vec![UiAction::new(ActionId::OPEN, tr("external.restart"))];
        it
    }

    fn outcome(&self, v: &Value) -> Result<ActionOutcome, PluginError> {
        if let Some(s) = v.as_str() {
            return Ok(match s {
                "keep_open" => ActionOutcome::KeepOpen,
                _ => ActionOutcome::Hide,
            });
        }
        if let Some(q) = str_of(v, "query") {
            return Ok(ActionOutcome::ReplaceQuery(q.to_owned()));
        }
        if let Some(n) = str_of(v, "notify") {
            return Ok(ActionOutcome::Notify(n.to_owned()));
        }
        if let Some(t) = str_of(v, "copy") {
            return crate::util::copy(t);
        }
        if let Some(target) = str_of(v, "open") {
            return rukes_platform::shell::launch(
                target,
                None,
                None,
                rukes_platform::shell::Verb::Open,
            )
            .map(|()| ActionOutcome::Hide)
            .map_err(|e| PluginError::Failed(e.to_string()));
        }
        if v.get("results").is_some() {
            let items = self.items(&v["results"]);
            return Ok(ActionOutcome::ShowResults(items));
        }
        Ok(ActionOutcome::Hide)
    }
}

fn action_label(id: &str) -> String {
    match id {
        ActionId::OPEN => "action.open".into(),
        ActionId::COPY_PATH => "action.copy_path".into(),
        ActionId::OPEN_LOCATION => "action.open_location".into(),
        other => other.to_owned(),
    }
}

impl Plugin for ExternalPlugin {
    fn metadata(&self) -> PluginMetadata {
        let m = &self.manifest;
        PluginMetadata {
            id: m.id.clone(),
            name: m.name.clone(),
            prefix: m.prefix.clone(),
            keywords: m.keywords.clone(),
            priority: m.priority,
            global: m.global,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        if !q.targeted && q.text.trim().is_empty() {
            return;
        }
        let params = json!({
            "query_id": q.id.0,
            "text": q.text,
            "raw": q.raw,
            "targeted": q.targeted,
        });
        let mut partial = |v: Value| {
            let items = self.items(&v);
            if !items.is_empty() {
                sink(items);
            }
        };
        let r = self.process.call_streaming(
            "query",
            params,
            QUERY_TIMEOUT,
            Some((cancel, q.id.0)),
            &mut partial,
        );
        match r {
            Ok(v) => sink(self.items(&v)),
            Err(CallError::Cancelled) => {}
            Err(e) if q.targeted => sink(vec![self.error_item(&e)]),
            Err(_) => sink(Vec::new()),
        }
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if item.id == "err" {
            self.process.restart();
            return Ok(ActionOutcome::Notify(trf(
                "external.restarted",
                &[("name", &self.manifest.name)],
            )));
        }
        let it: Value = serde_json::from_str(&item.payload).unwrap_or(Value::Null);
        let params = json!({ "item": it, "action": action });
        match self.process.call("action", params, ACTION_TIMEOUT) {
            Ok(v) => self.outcome(&v),
            Err(e) => Err(PluginError::Failed(format!("{}: {e}", self.manifest.name))),
        }
    }
}

/// Plugins externos instalados en una carpeta.
pub struct ExternalPlugins {
    /// Carpeta de plugins.
    pub root: PathBuf,
    /// Plugins válidos y activos.
    pub plugins: Vec<Arc<ExternalPlugin>>,
    /// Manifiestos con `enabled = false`.
    pub disabled: Vec<Manifest>,
    /// Errores de lectura (`archivo: error`).
    pub errors: Vec<String>,
}

impl ExternalPlugins {
    /// Lee la carpeta. Los procesos no arrancan hasta su primera consulta.
    pub fn load(root: &Path) -> Self {
        let (manifests, mut errors) = discover(root);
        let (mut plugins, mut disabled) = (Vec::new(), Vec::new());
        let mut seen = std::collections::BTreeSet::new();
        for m in manifests {
            if !m.enabled {
                disabled.push(m);
            } else if !seen.insert(m.id.clone()) {
                errors.push(format!("{}: id duplicado", m.dir.display()));
            } else {
                plugins.push(Arc::new(ExternalPlugin::new(m)));
            }
        }
        Self {
            root: root.to_owned(),
            plugins,
            disabled,
            errors,
        }
    }

    /// Plugins para el motor: los externos y la lista `plugins`.
    pub fn engine_plugins(self: &Arc<Self>) -> Vec<Arc<dyn Plugin>> {
        let mut out: Vec<Arc<dyn Plugin>> = self
            .plugins
            .iter()
            .map(|p| Arc::clone(p) as Arc<dyn Plugin>)
            .collect();
        out.push(Arc::new(PluginsListPlugin {
            set: Arc::clone(self),
        }));
        out
    }
}

/// `plugins`: lista los plugins externos con su estado y permisos; Enter reinicia uno.
pub struct PluginsListPlugin {
    set: Arc<ExternalPlugins>,
}

fn status(p: &ExternalPlugin) -> String {
    if p.process.is_disabled() {
        tr("external.status_disabled").into()
    } else if p.process.is_running() {
        tr("external.status_running").into()
    } else if p.process.failures() > 0 {
        trf("external.status_failures", &[("n", &p.process.failures())])
    } else {
        tr("external.status_idle").into()
    }
}

fn manifest_detail(m: &Manifest) -> String {
    let mut out = String::new();
    if let Some(d) = &m.description {
        out.push_str(d);
        out.push_str("\n\n");
    }
    let mut line = |k: &str, v: &str| {
        out.push_str(tr(k));
        out.push_str(": ");
        out.push_str(v);
        out.push('\n');
    };
    line("external.version", &m.version);
    if let Some(a) = &m.author {
        line("external.author", a);
    }
    let mut trig: Vec<String> = m.keywords.clone();
    if let Some(p) = &m.prefix {
        trig.insert(0, p.clone());
    }
    if m.global {
        trig.push(tr("external.global").into());
    }
    line("external.triggers", &trig.join(", "));
    line(
        "external.command",
        &format!("{} {}", m.executable, m.args.join(" ")),
    );
    let perms = m.permissions_label();
    line(
        "external.permissions",
        if perms.is_empty() {
            tr("external.no_permissions")
        } else {
            &perms
        },
    );
    line("external.folder", &m.dir.display().to_string());
    out
}

impl PluginsListPlugin {
    /// Resultados de la lista.
    pub fn search(&self, text: &str) -> Vec<ResultItem> {
        let mut f = Fuzzy::new(text);
        let mut out = Vec::new();
        let matches = |f: &mut Fuzzy, m: &Manifest| text.is_empty() || f.score(&m.name).is_some();
        for (i, p) in self.set.plugins.iter().enumerate() {
            let m = &p.manifest;
            if !matches(&mut f, m) {
                continue;
            }
            let mut it =
                ResultItem::new(format!("p:{}", m.id), format!("{} {}", m.name, m.version));
            let mut sub = vec![status(p)];
            let kw: Vec<&str> = m
                .prefix
                .iter()
                .chain(m.keywords.iter())
                .map(String::as_str)
                .collect();
            if !kw.is_empty() {
                sub.push(kw.join(", "));
            }
            let perms = m.permissions_label();
            if !perms.is_empty() {
                sub.push(perms);
            }
            if let Some(e) = p.process.last_error() {
                sub.push(e);
            }
            it.subtitle = sub.join(" · ");
            it.icon = icon_for(&m.dir, m.icon.as_deref());
            it.fuzzy = 1.0 - i as f32 * 0.001;
            it.detail = Some(manifest_detail(m));
            it.payload = m.id.clone();
            it.actions = vec![
                UiAction::new(ActionId::OPEN, tr("external.restart")),
                UiAction::new(ActionId::EDIT, tr("external.open_folder")),
            ];
            out.push(it);
        }
        for m in &self.set.disabled {
            if !matches(&mut f, m) {
                continue;
            }
            let mut it =
                ResultItem::new(format!("d:{}", m.id), format!("{} {}", m.name, m.version));
            it.subtitle = tr("external.status_off").into();
            it.icon = Icon::Named("plugin".into());
            it.fuzzy = 0.5;
            it.detail = Some(manifest_detail(m));
            it.payload = m.dir.display().to_string();
            it.actions = vec![UiAction::new(ActionId::EDIT, tr("external.open_folder"))];
            out.push(it);
        }
        for (i, e) in self.set.errors.iter().enumerate() {
            let mut it = ResultItem::new(format!("e:{i}"), tr("external.bad_manifest"));
            it.subtitle = e.clone();
            it.icon = Icon::Named("plugin".into());
            it.fuzzy = 0.4;
            it.actions = vec![UiAction::new(ActionId::EDIT, tr("external.open_folder"))];
            out.push(it);
        }
        if out.is_empty() {
            let mut it = ResultItem::new("none", tr("external.none"));
            it.subtitle = self.set.root.display().to_string();
            it.icon = Icon::Named("plugin".into());
            it.actions = vec![UiAction::new(ActionId::OPEN, tr("external.open_folder"))];
            out.push(it);
        }
        out
    }
}

impl Plugin for PluginsListPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "plugins".into(),
            name: tr("external.name").into(),
            prefix: None,
            keywords: vec!["plugins".into()],
            priority: 0.0,
            global: false,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(self.search(q.text.trim()));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let open_dir = |p: &Path| {
            let _ = std::fs::create_dir_all(p);
            rukes_platform::shell::launch(
                &p.to_string_lossy(),
                None,
                None,
                rukes_platform::shell::Verb::Open,
            )
            .map(|()| ActionOutcome::Hide)
            .map_err(|e| PluginError::Failed(e.to_string()))
        };
        let plugin = self
            .set
            .plugins
            .iter()
            .find(|p| p.manifest.id == item.payload);
        match (plugin, action) {
            (Some(p), ActionId::OPEN) => {
                p.process.restart();
                Ok(ActionOutcome::Notify(trf(
                    "external.restarted",
                    &[("name", &p.manifest.name)],
                )))
            }
            (Some(p), _) => open_dir(&p.manifest.dir),
            (None, _) if item.id.starts_with("d:") => open_dir(Path::new(&item.payload)),
            (None, _) => open_dir(&self.set.root),
        }
    }
}
