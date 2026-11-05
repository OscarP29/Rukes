//! Punto de entrada de Rukes: daemon residente con atajo global.
#![windows_subsystem = "windows"]

mod create_plugin;
mod host;
mod iconcache;
mod logging;
mod look;
mod preview;
mod settings;
mod settings_panel;
mod system_plugin;

use std::sync::Arc;
use std::time::Duration;

use rukes_config::paths::Paths;
use rukes_config::{Config, ConfigWatcher, DirWatcher};
use rukes_core::{Engine, Frecency, persist};
use rukes_platform::instance::{self, Instance};
use rukes_ui::UiEvent;

use crate::host::AppHost;

/// Cada cuánto se guarda la frecency si cambió.
const SAVE_EVERY: Duration = Duration::from_secs(30);

/// Órdenes de línea de comandos (sin ventana; el resultado va al log y al código de salida):
/// `--schema <archivo>`, `--export <carpeta>`, `--import <carpeta>`.
fn cli(paths: &Paths) -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, arg) = (
        args.first()?.as_str(),
        args.get(1).map(std::path::PathBuf::from),
    );
    let r: Result<String, String> = match (cmd, arg) {
        ("--schema", Some(f)) => std::fs::write(&f, rukes_config::transfer::json_schema())
            .map(|()| f.display().to_string())
            .map_err(|e| e.to_string()),
        ("--export", Some(d)) => rukes_config::transfer::export(paths, &d, &system_plugin::stamp())
            .map(|p| p.display().to_string())
            .map_err(|e| e.to_string()),
        ("--import", Some(d)) => rukes_config::transfer::import(paths, &d, &system_plugin::stamp())
            .map(|p| format!("copia de seguridad en {}", p.display())),
        _ => return None,
    };
    match r {
        Ok(msg) => {
            tracing::info!(cmd, %msg, "orden de línea de comandos");
            Some(0)
        }
        Err(e) => {
            tracing::error!(cmd, error = %e, "orden de línea de comandos");
            Some(1)
        }
    }
}

fn main() -> anyhow::Result<()> {
    let paths = Paths::detect();
    let _log = logging::init(&paths.logs_dir)?;
    if let Some(code) = cli(&paths) {
        std::process::exit(code);
    }
    if let Err(e) = rukes_config::transfer::write_schema(&paths) {
        tracing::warn!(%e, "no se pudo escribir config.schema.json");
    }
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        portable = paths.portable,
        "Rukes iniciando"
    );

    let (config, config_error) = match rukes_config::load(&paths.config_file()) {
        Ok(c) => (c, None),
        Err(e) => {
            tracing::error!(%e, "configuración no válida; se usan valores por defecto");
            (Config::default(), Some(e))
        }
    };
    rukes_core::i18n::set_lang_dir(Some(&paths.lang_dir()));
    if let Err(e) = rukes_core::i18n::set_language(
        &config.general.language,
        &rukes_platform::sysinfo::user_locale(),
    ) {
        tracing::warn!(%e, "archivo de idioma no válido");
    }

    // La UI se crea antes que el atajo para que la segunda instancia pueda pedir «mostrar».
    let ui_tx = rukes_ui::channel();
    let frecency = persist::load_json::<Frecency>(&paths.frecency_file())
        .filter(Frecency::is_compatible)
        .unwrap_or_default();
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 4));
    let engine = Arc::new(Engine::new(workers, settings::engine(&config), frecency));
    let host = Arc::new(AppHost::new(
        Arc::clone(&engine),
        ui_tx.clone(),
        paths.clone(),
        &config,
    ));

    let show_host = Arc::clone(&host);
    let _instance = match instance::acquire("Rukes", move || show_host.toggle(true))? {
        Instance::Primary(guard) => guard,
        Instance::Secondary => {
            tracing::info!("ya hay una instancia; se le pidió mostrarse");
            return Ok(());
        }
    };

    host.install_plugins(&config);
    host.refresh_look(true);
    engine.refresh_all();
    host.start_hotkey(&config.general.hotkey);
    host.start_tray();
    host::apply_autostart(&config);
    if let Some(e) = config_error {
        let _ = ui_tx.unbounded_send(UiEvent::Notice(format!(
            "{}: {e}",
            rukes_core::i18n::tr("error.config")
        )));
    }

    let watch_host = Arc::clone(&host);
    let _watcher = ConfigWatcher::spawn(&paths.config_file(), move |res| {
        watch_host.apply_config(res)
    })
    .inspect_err(|e| tracing::warn!(%e, "no se pudo vigilar config.toml"))
    .ok();
    let theme_host = Arc::clone(&host);
    let _themes = DirWatcher::spawn(&paths.themes_dir(), move || theme_host.refresh_look(true))
        .inspect_err(|e| tracing::warn!(%e, "no se pudo vigilar la carpeta de temas"))
        .ok();
    let lang_host = Arc::clone(&host);
    let _langs = DirWatcher::spawn(&paths.lang_dir(), move || lang_host.reload_lang())
        .inspect_err(|e| tracing::warn!(%e, "no se pudo vigilar la carpeta de idiomas"))
        .ok();

    let save_engine = Arc::clone(&engine);
    let frecency_file = paths.frecency_file();
    std::thread::Builder::new()
        .name("rukes-save".into())
        .spawn(move || {
            loop {
                std::thread::sleep(SAVE_EVERY);
                if let Err(e) = save_engine.save_frecency(&frecency_file) {
                    tracing::warn!(%e, "no se pudo guardar la frecency");
                }
            }
        })?;

    let (ws, private) = rukes_platform::sysinfo::memory_mb();
    tracing::info!(
        startup_ms = rukes_platform::sysinfo::process_age_ms(),
        working_set_mb = ws,
        private_mb = private,
        "listo"
    );

    let ui_config = host.ui_config();
    let result = rukes_ui::run(host.clone(), ui_tx, ui_config);
    host.shutdown();
    result.map_err(|e| anyhow::anyhow!("error de la UI: {e}"))
}
