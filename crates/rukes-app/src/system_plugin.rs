//! Comandos internos de Rukes: salir, recargar, abrir, exportar e importar la configuración.

use std::path::{Path, PathBuf};

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata, Query, ResultItem,
};
use rukes_platform::shell::{self, Verb};

/// Abre `config.toml` con su programa asociado o, si no hay, con el Bloc de notas.
pub fn open_config(file: &std::path::Path) -> rukes_platform::Result<()> {
    let file = file.to_string_lossy();
    shell::launch(&file, None, None, Verb::Open).or_else(|_| {
        shell::launch(
            "notepad.exe",
            Some(&format!("\"{file}\"")),
            None,
            Verb::Open,
        )
    })
}

/// Abre una carpeta de Rukes en el Explorador (la crea si aún no existe) o copia su ruta.
fn open_folder(dir: &Path, action: &str) -> Result<ActionOutcome, PluginError> {
    let fail = |e: String| PluginError::Failed(e);
    match action {
        rukes_core::ActionId::OPEN => {
            std::fs::create_dir_all(dir).map_err(|e| fail(e.to_string()))?;
            shell::launch(&dir.to_string_lossy(), None, None, Verb::Open)
                .map_err(|e| fail(e.to_string()))?;
            Ok(ActionOutcome::Hide)
        }
        rukes_core::ActionId::COPY_PATH => rukes_plugins::util::copy(&dir.to_string_lossy()),
        other => Err(PluginError::UnsupportedAction(other.into())),
    }
}

type Callback = Box<dyn Fn() + Send + Sync>;

/// Marca de tiempo para nombres de carpeta: `20261104-1530`.
pub fn stamp() -> String {
    let t = rukes_platform::time::local_now();
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    )
}

/// Carpeta Documentos del usuario (destino de «Exportar configuración»).
fn documents() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(|h| PathBuf::from(h).join("Documents"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// `importar <carpeta>` / `import <carpeta>` → la carpeta.
fn import_target(text: &str) -> Option<PathBuf> {
    let t = text.trim();
    let lower = t.to_lowercase();
    let rest = [
        "importar configuracion ",
        "importar configuración ",
        "import config ",
        "importar ",
        "import ",
    ]
    .iter()
    .find_map(|p| lower.starts_with(p).then(|| &t[p.len()..]))?;
    let rest = rest.trim().trim_matches('"');
    (!rest.is_empty()).then(|| rukes_script::args::expand_path(rest))
}

/// Plugin `rukes`.
pub struct SystemPlugin {
    pub config_file: PathBuf,
    pub paths: rukes_config::paths::Paths,
    /// Carpetas de Rukes que se abren desde el launcher: `(id, ruta)`; el título es la clave
    /// `system.folder_<id>`.
    pub folders: Vec<(&'static str, PathBuf)>,
    pub on_quit: Callback,
    pub on_reload: Callback,
}

/// `(id, clave de título, clave de subtítulo, palabras extra para buscar)`.
const COMMANDS: [(&str, &str, &str, &str); 6] = [
    (
        "settings",
        "system.settings",
        "system.settings_sub",
        "rukes settings ajustes preferencias opciones",
    ),
    (
        "quit",
        "system.quit",
        "system.quit_sub",
        "rukes exit quit salir cerrar",
    ),
    (
        "reload",
        "system.reload",
        "system.reload_sub",
        "rukes reload recargar reindex",
    ),
    (
        "config",
        "system.config",
        "",
        "rukes config configuracion editar toml",
    ),
    (
        "export",
        "system.export",
        "system.export_sub",
        "exportar export backup copia",
    ),
    (
        "import",
        "system.import",
        "system.import_sub",
        "importar import restaurar",
    ),
];

/// Palabras que piden la lista de carpetas (`carpetas`, `carpeta scripts`, `folder themes`).
const FOLDER_WORDS: [&str; 5] = ["carpeta", "carpetas", "folder", "folders", "dir"];

/// Si la consulta empieza por una palabra de [`FOLDER_WORDS`], lo que va detrás.
fn folder_filter(text: &str) -> Option<&str> {
    let t = text.trim_start();
    let word = t.split_whitespace().next()?;
    FOLDER_WORDS
        .iter()
        .any(|w| w.eq_ignore_ascii_case(word))
        .then(|| t[word.len()..].trim())
}

impl SystemPlugin {
    /// Resultados de las carpetas. Con `filter` (tras «carpeta») salen todas o las que
    /// coincidan; sin él, solo las que coincidan bien con el título.
    fn folder_items(&self, text: &str) -> Vec<ResultItem> {
        let filter = folder_filter(text);
        let mut f = Fuzzy::new(filter.unwrap_or(text));
        self.folders
            .iter()
            .filter_map(|(id, path)| {
                let title = tr(&format!("system.folder_{id}"));
                let score = match filter {
                    Some("") => 1.0,
                    Some(_) => f.score(title).or_else(|| f.score(id))?.max(0.5),
                    None => f.score(title).filter(|s| *s >= 0.5)?,
                };
                let mut it = ResultItem::new(format!("folder:{id}"), title);
                it.subtitle = path.display().to_string();
                it.icon = Icon::Named("folder".into());
                it.fuzzy = score;
                it.payload = path.to_string_lossy().into_owned();
                it.preview = path.is_dir().then(|| path.clone());
                it.actions.push(rukes_core::Action::new(
                    rukes_core::ActionId::COPY_PATH,
                    "action.copy_path",
                ));
                Some(it)
            })
            .collect()
    }
}

impl Plugin for SystemPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "rukes".into(),
            name: "Rukes".into(),
            prefix: None,
            keywords: Vec::new(),
            priority: -0.05,
            global: true,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        if let Some(dir) = import_target(&q.text) {
            let ok = dir.join("config.toml").is_file();
            let mut it = ResultItem::new(
                "import_from",
                trf("system.import_from", &[("dir", &dir.display())]),
            );
            it.subtitle = if ok {
                tr("system.import_from_sub").to_owned()
            } else {
                tr("system.import_missing").to_owned()
            };
            it.fuzzy = 1.0;
            it.payload = dir.to_string_lossy().into_owned();
            sink(vec![it]);
            return;
        }
        let folders = self.folder_items(&q.text);
        if folder_filter(&q.text).is_some() {
            sink(folders);
            return;
        }
        let mut f = Fuzzy::new(&q.text);
        let mut items: Vec<ResultItem> = COMMANDS
            .iter()
            .filter_map(|(id, title, sub, extra)| {
                let title = tr(title);
                // Las palabras extra solo cuentan si coinciden de verdad (umbral alto).
                let extra_score = extra
                    .split(' ')
                    .filter_map(|w| f.score(w))
                    .fold(0.0f32, f32::max);
                let score = f.score(title).unwrap_or(0.0).max(if extra_score >= 0.9 {
                    extra_score
                } else {
                    0.0
                });
                if score <= 0.0 {
                    return None;
                }
                let mut it = ResultItem::new(*id, title);
                it.subtitle = if sub.is_empty() {
                    self.config_file.display().to_string()
                } else if *id == "export" {
                    trf(sub, &[("dir", &documents().display())])
                } else {
                    tr(sub).to_owned()
                };
                it.fuzzy = score;
                it.icon = Icon::None;
                Some(it)
            })
            .collect();
        items.extend(folders);
        sink(items);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if item.id.starts_with("folder:") {
            return open_folder(Path::new(&item.payload), action);
        }
        if action != rukes_core::ActionId::OPEN {
            return Err(PluginError::UnsupportedAction(action.into()));
        }
        match item.id.as_str() {
            "quit" => {
                (self.on_quit)();
                Ok(ActionOutcome::Hide)
            }
            "settings" => Ok(ActionOutcome::OpenSettings),
            "reload" => {
                (self.on_reload)();
                Ok(ActionOutcome::Hide)
            }
            "config" => open_config(&self.config_file)
                .map(|()| ActionOutcome::Hide)
                .map_err(|e| PluginError::Failed(e.to_string())),
            "export" => {
                let out = rukes_config::transfer::export(&self.paths, &documents(), &stamp())
                    .map_err(|e| PluginError::Failed(e.to_string()))?;
                let _ = shell::launch(&out.to_string_lossy(), None, None, Verb::Open);
                Ok(ActionOutcome::Notify(trf(
                    "system.exported",
                    &[("dir", &out.display())],
                )))
            }
            // Sin carpeta: se pide escribirla.
            "import" => Ok(ActionOutcome::ReplaceQuery(format!(
                "{} ",
                tr("system.import_word")
            ))),
            "import_from" => {
                let backup =
                    rukes_config::transfer::import(&self.paths, Path::new(&item.payload), &stamp())
                        .map_err(PluginError::Failed)?;
                Ok(ActionOutcome::Notify(trf(
                    "system.imported",
                    &[("dir", &backup.display())],
                )))
            }
            other => Err(PluginError::Failed(format!("comando desconocido: {other}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin() -> SystemPlugin {
        let paths = rukes_config::paths::Paths::under(Path::new("C:/r"), true);
        SystemPlugin {
            config_file: paths.config_file(),
            folders: vec![
                ("workflows", paths.workflows_dir()),
                ("scripts", PathBuf::from("C:/r/scripts")),
                ("themes", paths.themes_dir()),
            ],
            paths,
            on_quit: Box::new(|| {}),
            on_reload: Box::new(|| {}),
        }
    }

    #[test]
    fn folder_word_lists_and_filters() {
        assert_eq!(folder_filter("carpetas"), Some(""));
        assert_eq!(folder_filter("Folder  scr"), Some("scr"));
        assert_eq!(folder_filter("carpet"), None);
        let p = plugin();
        assert_eq!(p.folder_items("carpetas").len(), 3);
        let only = p.folder_items("carpeta scri");
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].id, "folder:scripts");
        assert_eq!(only[0].payload, "C:/r/scripts");
    }
}
