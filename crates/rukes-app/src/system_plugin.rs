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
    pub on_quit: Callback,
    pub on_reload: Callback,
}

/// `(id, clave de título, clave de subtítulo, palabras extra para buscar)`.
const COMMANDS: [(&str, &str, &str, &str); 5] = [
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
        "rukes config configuracion settings ajustes",
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
        let mut f = Fuzzy::new(&q.text);
        let items = COMMANDS
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
        sink(items);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if action != rukes_core::ActionId::OPEN {
            return Err(PluginError::UnsupportedAction(action.into()));
        }
        match item.id.as_str() {
            "quit" => {
                (self.on_quit)();
                Ok(ActionOutcome::Hide)
            }
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
