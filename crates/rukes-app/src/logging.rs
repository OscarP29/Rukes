//! Inicialización de `tracing` con salida a consola y a archivo rotativo (RNF-014).

use std::path::Path;

use anyhow::Context as _;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Número máximo de archivos de log que se conservan.
const MAX_LOG_FILES: usize = 5;

/// Configura los subscribers. El `WorkerGuard` devuelto debe vivir hasta el final de `main`
/// para que los logs pendientes se vuelquen al salir. Nivel con `RUKES_LOG` (por defecto `info`).
pub fn init(dir: &Path) -> anyhow::Result<WorkerGuard> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("no se pudo crear el directorio de logs {}", dir.display()))?;

    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("rukes")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .context("no se pudo crear el archivo de log")?;
    let (file_writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_env("RUKES_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,wgpu=warn,iced=warn,naga=warn,cosmic_text=warn"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(fmt::layer().with_ansi(false).with_writer(file_writer))
        .try_init()
        .context("no se pudo inicializar tracing")?;

    Ok(guard)
}
