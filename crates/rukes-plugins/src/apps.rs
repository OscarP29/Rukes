//! Plugin de aplicaciones (RF-030 a RF-032).
//!
//! El índice se carga de la caché al arrancar (instantáneo) y se reconstruye en segundo plano
//! con `refresh` (varios segundos: resolver accesos directos es lento).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rukes_core::i18n::tr;
use rukes_core::{
    Action, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata,
    Query, ResultItem, persist,
};
use rukes_platform::apps::{self as scan, AppKind};
use rukes_platform::shell::{self, Verb};
use serde::{Deserialize, Serialize};

const CACHE_VERSION: u32 = 1;
/// Penalización de los ejecutables del PATH frente a apps «de verdad».
const PATH_EXE_FACTOR: f32 = 0.85;
const MAX_RESULTS: usize = 50;

/// Tipo de app serializable (espejo de [`AppKind`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    /// Acceso directo.
    Shortcut {
        /// `.lnk`.
        lnk: PathBuf,
        /// Destino.
        target: Option<PathBuf>,
    },
    /// App empaquetada.
    Packaged {
        /// AUMID.
        aumid: String,
    },
    /// Ejecutable del PATH.
    Exe {
        /// Ruta.
        path: PathBuf,
    },
}

impl From<AppKind> for Kind {
    fn from(k: AppKind) -> Self {
        match k {
            AppKind::Shortcut { lnk, target } => Self::Shortcut { lnk, target },
            AppKind::Packaged { aumid } => Self::Packaged { aumid },
            AppKind::Exe { path } => Self::Exe { path },
        }
    }
}

impl From<&Kind> for AppKind {
    fn from(k: &Kind) -> Self {
        match k.clone() {
            Kind::Shortcut { lnk, target } => Self::Shortcut { lnk, target },
            Kind::Packaged { aumid } => Self::Packaged { aumid },
            Kind::Exe { path } => Self::Exe { path },
        }
    }
}

/// Una app del índice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    /// Nombre visible.
    pub name: String,
    /// Tipo.
    pub kind: Kind,
}

impl App {
    /// Id estable para la frecency.
    fn id(&self) -> String {
        match &self.kind {
            Kind::Shortcut { lnk, .. } => format!("lnk:{}", lnk.display()).to_lowercase(),
            Kind::Packaged { aumid } => format!("pkg:{aumid}").to_lowercase(),
            Kind::Exe { path } => format!("exe:{}", path.display()).to_lowercase(),
        }
    }

    fn rank(&self) -> u8 {
        match self.kind {
            Kind::Shortcut { .. } => 0,
            Kind::Packaged { .. } => 1,
            Kind::Exe { .. } => 2,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    apps: Vec<App>,
}

/// Ajustes del plugin.
#[derive(Debug, Clone, Default)]
pub struct AppsSettings {
    /// Alias → nombre de app.
    pub aliases: BTreeMap<String, String>,
    /// Incluir ejecutables del PATH.
    pub include_path: bool,
    /// Prioridad del plugin.
    pub priority: f32,
}

/// Plugin de aplicaciones.
pub struct AppsPlugin {
    index: RwLock<Arc<Vec<App>>>,
    /// alias en minúsculas → nombre de app en minúsculas.
    aliases: HashMap<String, String>,
    settings: AppsSettings,
    cache_file: PathBuf,
}

/// Elimina duplicados por nombre (gana acceso directo > empaquetada > PATH) y los ejecutables
/// del PATH que ya son destino de un acceso directo.
pub fn dedupe(mut apps: Vec<App>) -> Vec<App> {
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.rank().cmp(&b.rank()))
    });
    let targets: HashSet<String> = apps
        .iter()
        .filter_map(|a| match &a.kind {
            Kind::Shortcut {
                target: Some(t), ..
            } => Some(t.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    let mut seen = HashSet::new();
    apps.into_iter()
        .filter(|a| {
            if let Kind::Exe { path } = &a.kind
                && targets.contains(&path.to_string_lossy().to_lowercase())
            {
                return false;
            }
            seen.insert(a.name.to_lowercase())
        })
        .collect()
}

impl AppsPlugin {
    /// Crea el plugin con el índice de la caché (si existe).
    pub fn new(settings: AppsSettings, cache_file: PathBuf) -> Self {
        let cached = persist::load_json::<Cache>(&cache_file)
            .filter(|c| c.version == CACHE_VERSION)
            .map(|c| c.apps)
            .unwrap_or_default();
        Self::with_apps(settings, cache_file, cached)
    }

    /// Crea el plugin con un índice dado (tests).
    pub fn with_apps(settings: AppsSettings, cache_file: PathBuf, apps: Vec<App>) -> Self {
        let aliases = settings
            .aliases
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.to_lowercase()))
            .collect();
        Self {
            index: RwLock::new(Arc::new(apps)),
            aliases,
            settings,
            cache_file,
        }
    }

    /// Número de apps indexadas.
    pub fn len(&self) -> usize {
        self.index.read().map(|i| i.len()).unwrap_or(0)
    }

    /// `true` si el índice está vacío (primer arranque sin caché).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn scan(&self) -> Vec<App> {
        let mut apps: Vec<App> = scan::start_menu_shortcuts()
            .into_iter()
            .chain(scan::packaged_apps())
            .map(|e| App {
                name: e.name,
                kind: e.kind.into(),
            })
            .collect();
        if self.settings.include_path {
            apps.extend(scan::path_executables().into_iter().map(|e| App {
                name: e.name,
                kind: e.kind.into(),
            }));
        }
        dedupe(apps)
    }

    fn to_item(app: &App, fuzzy: f32, alias: bool) -> ResultItem {
        let kind = AppKind::from(&app.kind);
        let mut it = ResultItem::new(app.id(), app.name.clone());
        it.fuzzy = fuzzy;
        it.alias_match = alias;
        it.icon = Icon::Shell(kind.icon_source());
        it.subtitle = match (&app.kind, kind.file_path()) {
            (Kind::Packaged { .. }, _) => tr("apps.packaged").to_owned(),
            (_, Some(p)) => p.display().to_string(),
            (_, None) => String::new(),
        };
        if !matches!(app.kind, Kind::Packaged { .. }) {
            it.actions
                .push(Action::new(ActionId::RUN_AS_ADMIN, "action.run_as_admin"));
        }
        if kind.file_path().is_some() {
            it.actions
                .push(Action::new(ActionId::OPEN_LOCATION, "action.open_location"));
            it.actions
                .push(Action::new(ActionId::COPY_PATH, "action.copy_path"));
        }
        it.payload = serde_json::to_string(&app.kind).unwrap_or_default();
        it
    }

    /// Búsqueda pura sobre el índice (sin E/S).
    pub fn search(&self, text: &str, cancel: &CancelToken) -> Vec<ResultItem> {
        let index = self
            .index
            .read()
            .map(|i| Arc::clone(&i))
            .unwrap_or_default();
        let q = text.trim().to_lowercase();
        let alias_target = self.aliases.get(&q);
        let mut f = Fuzzy::new(text);
        let mut out = Vec::new();
        for (n, app) in index.iter().enumerate() {
            if n % 512 == 0 && cancel.is_cancelled() {
                return Vec::new();
            }
            let alias = alias_target.is_some_and(|t| *t == app.name.to_lowercase());
            let score = if alias { Some(1.0) } else { f.score(&app.name) };
            let Some(mut s) = score else { continue };
            if matches!(app.kind, Kind::Exe { .. }) {
                s *= PATH_EXE_FACTOR;
            }
            out.push(Self::to_item(app, s, alias));
        }
        out.sort_by(|a, b| b.fuzzy.total_cmp(&a.fuzzy));
        out.truncate(MAX_RESULTS);
        out
    }
}

impl Plugin for AppsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "apps".into(),
            name: "Aplicaciones".into(),
            prefix: None,
            keywords: Vec::new(),
            priority: self.settings.priority,
            global: true,
        }
    }

    fn query(&self, query: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        if query.text.is_empty() {
            return;
        }
        sink(self.search(&query.text, cancel));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let kind: Kind = serde_json::from_str(&item.payload)
            .map_err(|e| PluginError::Failed(format!("payload no válido: {e}")))?;
        let kind = AppKind::from(&kind);
        let fail = |e: rukes_platform::PlatformError| PluginError::Failed(e.to_string());
        match action {
            ActionId::OPEN => shell::launch(&kind.launch_target(), None, None, Verb::Open)
                .map(|()| ActionOutcome::Hide)
                .map_err(fail),
            ActionId::RUN_AS_ADMIN => shell::launch(&kind.launch_target(), None, None, Verb::RunAs)
                .map(|()| ActionOutcome::Hide)
                .map_err(fail),
            ActionId::OPEN_LOCATION => {
                let p = kind
                    .file_path()
                    .ok_or_else(|| PluginError::UnsupportedAction(action.into()))?;
                shell::open_location(p)
                    .map(|()| ActionOutcome::Hide)
                    .map_err(fail)
            }
            ActionId::COPY_PATH => {
                let p = kind
                    .file_path()
                    .ok_or_else(|| PluginError::UnsupportedAction(action.into()))?;
                rukes_platform::clipboard::set_text(&p.display().to_string())
                    .map(|()| ActionOutcome::Notify(tr("notify.copied").to_owned()))
                    .map_err(fail)
            }
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }

    fn refresh(&self) {
        let started = std::time::Instant::now();
        let apps = self.scan();
        let n = apps.len();
        if let Err(e) = persist::save_json(
            &self.cache_file,
            &Cache {
                version: CACHE_VERSION,
                apps: apps.clone(),
            },
        ) {
            tracing::warn!(error = %e, "no se pudo guardar la caché de apps");
        }
        if let Ok(mut i) = self.index.write() {
            *i = Arc::new(apps);
        }
        tracing::info!(
            apps = n,
            ms = started.elapsed().as_millis() as u64,
            "índice de apps actualizado"
        );
    }
}

/// Ruta de la caché del índice dentro del directorio de cachés.
pub fn cache_file(cache_dir: &Path) -> PathBuf {
    cache_dir.join("apps.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lnk(name: &str, target: Option<&str>) -> App {
        App {
            name: name.into(),
            kind: Kind::Shortcut {
                lnk: format!("C:\\Start\\{name}.lnk").into(),
                target: target.map(Into::into),
            },
        }
    }

    fn pkg(name: &str) -> App {
        App {
            name: name.into(),
            kind: Kind::Packaged {
                aumid: format!("{name}!App"),
            },
        }
    }

    fn exe(name: &str, path: &str) -> App {
        App {
            name: name.into(),
            kind: Kind::Exe { path: path.into() },
        }
    }

    fn plugin(apps: Vec<App>, aliases: &[(&str, &str)]) -> AppsPlugin {
        AppsPlugin::with_apps(
            AppsSettings {
                aliases: aliases
                    .iter()
                    .map(|(a, b)| ((*a).into(), (*b).into()))
                    .collect(),
                include_path: true,
                priority: 0.0,
            },
            PathBuf::from("unused.json"),
            dedupe(apps),
        )
    }

    #[test]
    fn dedupe_prefers_shortcut_and_drops_shadowed_exes() {
        let apps = dedupe(vec![
            pkg("Calculator"),
            lnk("Calculator", None),
            exe("code", "C:\\VSCode\\code.exe"),
            lnk("Visual Studio Code", Some("C:\\VSCode\\Code.exe")),
            exe("git", "C:\\Git\\git.exe"),
        ]);
        assert_eq!(apps.len(), 3);
        assert!(matches!(
            apps.iter().find(|a| a.name == "Calculator").unwrap().kind,
            Kind::Shortcut { .. }
        ));
        assert!(!apps.iter().any(|a| a.name == "code"));
        assert!(apps.iter().any(|a| a.name == "git"));
    }

    #[test]
    fn use_case_1_chr_finds_chrome_first() {
        let p = plugin(
            vec![
                lnk("Google Chrome", Some("C:\\Chrome\\chrome.exe")),
                lnk("Character Map", None),
                pkg("Clipchamp"),
            ],
            &[],
        );
        let r = p.search("chr", &CancelToken::new());
        assert_eq!(r[0].title, "Google Chrome");
        assert!(r[0].has_action(ActionId::RUN_AS_ADMIN));
        assert!(r[0].has_action(ActionId::OPEN_LOCATION));
    }

    #[test]
    fn alias_matches_exactly() {
        let p = plugin(
            vec![lnk("Google Chrome", None), lnk("Calculator", None)],
            &[("c", "Google Chrome")],
        );
        let r = p.search("c", &CancelToken::new());
        let chrome = r.iter().find(|i| i.title == "Google Chrome").unwrap();
        assert!(chrome.alias_match);
        assert_eq!(chrome.fuzzy, 1.0);
    }

    #[test]
    fn packaged_apps_have_no_admin_or_location() {
        let p = plugin(vec![pkg("Spotify")], &[]);
        let r = p.search("spot", &CancelToken::new());
        assert!(!r[0].has_action(ActionId::RUN_AS_ADMIN));
        assert!(!r[0].has_action(ActionId::COPY_PATH));
        assert_eq!(
            AppKind::from(&serde_json::from_str::<Kind>(&r[0].payload).unwrap()).launch_target(),
            "shell:AppsFolder\\Spotify!App"
        );
    }

    #[test]
    fn path_exes_rank_below_equal_matches() {
        let p = plugin(
            vec![exe("notes", "C:\\bin\\notes.exe"), lnk("Notes", None)],
            &[],
        );
        // Mismo nombre → dedupe deja el acceso directo.
        assert_eq!(p.len(), 1);
        let p = plugin(
            vec![exe("paint", "C:\\bin\\paint.exe"), lnk("Paint 3D", None)],
            &[],
        );
        let r = p.search("paint", &CancelToken::new());
        let e = r.iter().find(|i| i.title == "paint").unwrap();
        assert!(e.fuzzy < 1.0);
    }

    #[test]
    fn cancelled_query_returns_nothing() {
        let p = plugin(vec![lnk("Google Chrome", None)], &[]);
        let c = CancelToken::new();
        c.cancel();
        assert!(p.search("chr", &c).is_empty());
    }

    #[test]
    fn cache_roundtrip() {
        let dir = std::env::temp_dir().join(format!("rukes-apps-{}", std::process::id()));
        let file = cache_file(&dir);
        persist::save_json(
            &file,
            &Cache {
                version: CACHE_VERSION,
                apps: vec![lnk("Google Chrome", None)],
            },
        )
        .unwrap();
        let p = AppsPlugin::new(AppsSettings::default(), file);
        assert_eq!(p.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
