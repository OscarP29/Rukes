//! Cambiar a una ventana abierta (RF-050).

use rukes_core::i18n::tr;
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata, Query,
    ResultItem,
};
use rukes_platform::winlist::{self, WindowInfo};

/// Plugin de ventanas.
pub struct WindowsPlugin {
    /// Prioridad configurada (se suma a la base, por debajo de las apps).
    pub priority: f32,
}

fn exe_name(w: &WindowInfo) -> String {
    w.exe
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Construye los resultados a partir de una lista de ventanas.
pub fn search(windows: &[WindowInfo], text: &str, targeted: bool) -> Vec<ResultItem> {
    let mut f = Fuzzy::new(text);
    windows
        .iter()
        .enumerate()
        .filter_map(|(i, w)| {
            let exe = exe_name(w);
            let score = if text.is_empty() {
                // `win` a secas: todas, en orden Z.
                targeted.then_some(1.0 - i as f32 * 0.001)?
            } else {
                let by_title = f.score(&w.title);
                let by_exe = f.score(&exe).map(|s| s * 0.9);
                by_title.into_iter().chain(by_exe).reduce(f32::max)?
            };
            let mut it = ResultItem::new(format!("hwnd:{}", w.hwnd), w.title.clone());
            let state = if w.minimized {
                tr("windows.minimized")
            } else {
                tr("windows.open")
            };
            it.subtitle = if exe.is_empty() {
                state.to_owned()
            } else {
                format!("{exe} · {state}")
            };
            it.icon = w
                .exe
                .clone()
                .map_or(Icon::Named("window".into()), Icon::File);
            it.fuzzy = score;
            it.payload = w.hwnd.to_string();
            Some(it)
        })
        .collect()
}

impl Plugin for WindowsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "windows".into(),
            name: tr("windows.name").into(),
            prefix: None,
            keywords: vec!["win".into(), "ventana".into(), "ventanas".into()],
            // Por debajo de las apps: al escribir «chrome» primero sale la app.
            priority: -0.1 + self.priority,
            global: true,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(search(&winlist::list(), &q.text, q.targeted));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if action != ActionId::OPEN {
            return Err(PluginError::UnsupportedAction(action.into()));
        }
        let hwnd = item
            .payload
            .parse()
            .map_err(|_| PluginError::Failed("ventana no válida".into()))?;
        if winlist::activate(hwnd) {
            Ok(ActionOutcome::Hide)
        } else {
            Err(PluginError::Failed(tr("windows.gone").into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn w(hwnd: isize, title: &str, exe: &str) -> WindowInfo {
        WindowInfo {
            hwnd,
            title: title.into(),
            pid: 1,
            exe: Some(PathBuf::from(format!("C:\\x\\{exe}.exe"))),
            minimized: false,
        }
    }

    #[test]
    fn matches_title_and_exe() {
        let list = [
            w(1, "README.md - Visual Studio Code", "Code"),
            w(2, "YouTube - Google Chrome", "chrome"),
        ];
        let r = search(&list, "youtube", false);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].payload, "2");
        let r = search(&list, "code", false);
        assert_eq!(r[0].payload, "1");
        // Global sin texto: nada; dirigido sin texto: todas.
        assert!(search(&list, "", false).is_empty());
        assert_eq!(search(&list, "", true).len(), 2);
    }
}
