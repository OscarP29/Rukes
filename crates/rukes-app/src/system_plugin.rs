//! Comandos internos de Rukes: salir, recargar y abrir la configuración.

use std::path::PathBuf;

use rukes_core::i18n::tr;
use rukes_core::{
    ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata, Query, ResultItem,
};
use rukes_platform::shell::{self, Verb};

type Callback = Box<dyn Fn() + Send + Sync>;

/// Plugin `rukes`.
pub struct SystemPlugin {
    pub config_file: PathBuf,
    pub on_quit: Callback,
    pub on_reload: Callback,
}

/// `(id, clave de título, clave de subtítulo, palabras extra para buscar)`.
const COMMANDS: [(&str, &str, &str, &str); 3] = [
    (
        "quit",
        "system.quit",
        "system.quit.sub",
        "rukes exit quit salir cerrar",
    ),
    (
        "reload",
        "system.reload",
        "system.reload.sub",
        "rukes reload recargar reindex",
    ),
    (
        "config",
        "system.config",
        "",
        "rukes config configuracion settings ajustes",
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
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
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
            "config" => {
                let file = self.config_file.to_string_lossy();
                // Sin asociación para .toml, `open` falla: se usa el Bloc de notas.
                shell::launch(&file, None, None, Verb::Open)
                    .or_else(|_| {
                        shell::launch(
                            "notepad.exe",
                            Some(&format!("\"{file}\"")),
                            None,
                            Verb::Open,
                        )
                    })
                    .map(|()| ActionOutcome::Hide)
                    .map_err(|e| PluginError::Failed(e.to_string()))
            }
            other => Err(PluginError::Failed(format!("comando desconocido: {other}"))),
        }
    }
}
