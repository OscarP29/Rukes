//! Elementos fijados: apps, archivos, carpetas, scripts o workflows que se muestran con la
//! búsqueda vacía para abrirlos de un toque (Ctrl+S fija o suelta, Ctrl+1…9 abre, Alt+↑ ↓ ordena).
//!
//! Se guarda una copia del resultado (con su `payload`) y se ejecuta con el plugin que lo dio,
//! así que vale para cualquier plugin cuyos resultados se puedan ejecutar fuera de su búsqueda.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::{Action, ActionId, Icon, ResultItem};

/// Fijados como máximo.
pub const MAX_PINS: usize = 30;

/// Copia de un resultado fijado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pinned {
    /// Plugin que lo dio.
    pub plugin_id: String,
    /// Id dentro del plugin.
    pub id: String,
    /// Título.
    pub title: String,
    /// Subtítulo.
    #[serde(default)]
    pub subtitle: String,
    /// Icono.
    #[serde(default = "no_icon")]
    pub icon: Icon,
    /// Datos del plugin para ejecutarlo.
    #[serde(default)]
    pub payload: String,
    /// Archivo o carpeta de la previsualización.
    #[serde(default)]
    pub preview: Option<PathBuf>,
    /// `(id, título)` de sus acciones, sin la de fijar.
    #[serde(default)]
    pub actions: Vec<(String, String)>,
}

fn no_icon() -> Icon {
    Icon::None
}

impl Pinned {
    /// Copia de un resultado.
    pub fn from_item(it: &ResultItem) -> Self {
        Self {
            plugin_id: it.plugin_id.clone(),
            id: it.id.clone(),
            title: it.title.clone(),
            subtitle: it.subtitle.clone(),
            icon: it.icon.clone(),
            payload: it.payload.clone(),
            preview: it.preview.clone(),
            actions: it
                .actions
                .iter()
                .filter(|a| a.id.as_str() != ActionId::PIN)
                .map(|a| (a.id.0.clone(), a.title.clone()))
                .collect(),
        }
    }

    /// Clave global (`plugin:id`), la misma que [`ResultItem::key`].
    pub fn key(&self) -> String {
        format!("{}:{}", self.plugin_id, self.id)
    }

    /// El resultado otra vez, con la acción de soltar al final.
    pub fn to_item(&self) -> ResultItem {
        let mut it = ResultItem::new(self.id.clone(), self.title.clone());
        it.plugin_id.clone_from(&self.plugin_id);
        it.subtitle.clone_from(&self.subtitle);
        it.icon = self.icon.clone();
        it.payload.clone_from(&self.payload);
        it.preview.clone_from(&self.preview);
        if !self.actions.is_empty() {
            it.actions = self
                .actions
                .iter()
                .map(|(id, title)| Action::new(id, title.clone()))
                .collect();
        }
        it.actions.push(Action::new(ActionId::PIN, "pins.unpin"));
        it
    }
}

/// Lista de fijados guardada en un JSON.
#[derive(Debug)]
pub struct Pins {
    path: PathBuf,
    items: Mutex<Vec<Pinned>>,
}

impl Pins {
    /// Lee los fijados de `path` (vacío si no existe).
    pub fn load(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            items: Mutex::new(crate::persist::load_json(path).unwrap_or_default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Pinned>> {
        self.items.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn save(&self, items: &[Pinned]) {
        if let Err(e) = crate::persist::save_json(&self.path, &items) {
            tracing::warn!(error = %e, "no se pudieron guardar los fijados");
        }
    }

    /// Los fijados, en su orden, listos para mostrar.
    pub fn items(&self) -> Vec<ResultItem> {
        self.lock().iter().map(Pinned::to_item).collect()
    }

    /// `true` si el resultado con esa clave está fijado.
    pub fn contains(&self, key: &str) -> bool {
        self.lock().iter().any(|p| p.key() == key)
    }

    /// Fija el resultado o lo suelta si ya estaba. Devuelve si queda fijado.
    pub fn toggle(&self, it: &ResultItem) -> Result<bool, String> {
        let mut items = self.lock();
        let key = it.key();
        let pinned = if let Some(i) = items.iter().position(|p| p.key() == key) {
            items.remove(i);
            false
        } else {
            if items.len() >= MAX_PINS {
                return Err(crate::i18n::tr("pins.full").to_owned());
            }
            items.push(Pinned::from_item(it));
            true
        };
        self.save(&items);
        Ok(pinned)
    }

    /// Mueve un fijado `delta` puestos. Devuelve si se movió.
    pub fn shift(&self, key: &str, delta: isize) -> bool {
        let mut items = self.lock();
        let Some(i) = items.iter().position(|p| p.key() == key) else {
            return false;
        };
        let j = i as isize + delta;
        if j < 0 || j >= items.len() as isize {
            return false;
        }
        items.swap(i, j as usize);
        self.save(&items);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(plugin: &str, id: &str) -> ResultItem {
        let mut it = ResultItem::new(id, format!("Título {id}"));
        it.plugin_id = plugin.into();
        it.payload = format!("{{\"p\":\"{id}\"}}");
        it.icon = Icon::File(PathBuf::from("C:/x.exe"));
        it.actions
            .push(Action::new(ActionId::COPY_PATH, "action.copy_path"));
        it.actions.push(Action::new(ActionId::PIN, "pins.pin"));
        it
    }

    #[test]
    fn toggle_persist_and_reorder() {
        let dir = std::env::temp_dir().join(format!("rukes-pins-{}", std::process::id()));
        let path = dir.join("pinned.json");
        let pins = Pins::load(&path);
        assert!(pins.items().is_empty());
        assert_eq!(pins.toggle(&item("apps", "a")), Ok(true));
        assert_eq!(pins.toggle(&item("files", "b")), Ok(true));
        assert!(pins.contains("apps:a"));

        let again = Pins::load(&path);
        let list = again.items();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].plugin_id, "apps");
        assert_eq!(list[0].payload, "{\"p\":\"a\"}");
        assert_eq!(list[0].icon, Icon::File(PathBuf::from("C:/x.exe")));
        let ids: Vec<_> = list[0].actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            ["open", "copy_path", "pin"],
            "una sola acción de fijar"
        );
        assert_eq!(list[0].actions[2].title, "pins.unpin");

        assert!(again.shift("files:b", -1));
        assert!(!again.shift("files:b", -1), "ya es el primero");
        assert_eq!(again.items()[0].id, "b");
        assert_eq!(again.toggle(&list[0]), Ok(false));
        assert_eq!(Pins::load(&path).items().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
