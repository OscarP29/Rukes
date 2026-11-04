//! Textos de interfaz en español e inglés (sección 2.5). En la fase 4 pasarán a archivos de
//! recursos (RF-211); la API `tr` se mantiene.

use std::sync::atomic::{AtomicU8, Ordering};

/// Idioma de la interfaz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Español.
    Es,
    /// Inglés.
    En,
}

static LANG: AtomicU8 = AtomicU8::new(0);

/// Fija el idioma. `auto` usa el del sistema (`sys_lang`, p. ej. `es-CO`).
pub fn set_language(config: &str, sys_lang: &str) {
    let pick = match config {
        "es" => Lang::Es,
        "en" => Lang::En,
        _ if sys_lang.to_ascii_lowercase().starts_with("es") => Lang::Es,
        _ => Lang::En,
    };
    LANG.store(pick as u8, Ordering::Relaxed);
}

/// Idioma actual.
pub fn language() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Es
    }
}

/// Traduce una clave. Si no existe, devuelve la propia clave.
pub fn tr(key: &str) -> &str {
    let (es, en) = match key {
        "search.placeholder" => (
            "Buscar apps, archivos y comandos…",
            "Search apps, files and commands…",
        ),
        "search.no_results" => ("Sin resultados", "No results"),
        "action.open" => ("Abrir", "Open"),
        "action.run_as_admin" => ("Abrir como administrador", "Run as administrator"),
        "action.open_location" => ("Abrir ubicación", "Open file location"),
        "action.copy_path" => ("Copiar ruta", "Copy path"),
        "action.open_with" => ("Abrir con…", "Open with…"),
        "action.terminal_here" => ("Abrir terminal aquí", "Open terminal here"),
        "apps.packaged" => ("Aplicación de la Tienda", "Store app"),
        "apps.path" => ("Ejecutable en PATH", "Executable on PATH"),
        "notify.copied" => ("Copiado al portapapeles", "Copied to clipboard"),
        "error.config" => ("Error en la configuración", "Configuration error"),
        "error.hotkey" => (
            "No se pudo registrar el atajo",
            "Could not register the hotkey",
        ),
        "system.quit" => ("Salir de Rukes", "Quit Rukes"),
        "system.quit.sub" => (
            "Cierra el launcher por completo",
            "Close the launcher completely",
        ),
        "system.reload" => ("Recargar Rukes", "Reload Rukes"),
        "system.reload.sub" => ("Vuelve a indexar las aplicaciones", "Re-index applications"),
        "system.config" => ("Abrir configuración de Rukes", "Open Rukes settings"),
        "error.action" => ("No se pudo completar la acción", "The action failed"),
        "calc.enter_copies" => ("Enter copia", "Enter copies"),
        "tray.show" => ("Mostrar", "Show"),
        "tray.settings" => ("Configuración", "Settings"),
        "tray.reload" => ("Recargar índices", "Reload indexes"),
        "tray.pause" => ("Pausar atajo", "Pause hotkey"),
        "tray.resume" => ("Reanudar atajo", "Resume hotkey"),
        "tray.quit" => ("Salir", "Quit"),
        _ => return key,
    };
    match language() {
        Lang::Es => es,
        Lang::En => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_falls_back() {
        set_language("en", "es-CO");
        assert_eq!(tr("action.open"), "Open");
        set_language("auto", "es-CO");
        assert_eq!(tr("action.open"), "Abrir");
        assert_eq!(tr("missing.key"), "missing.key");
    }
}
