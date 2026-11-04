//! Punto de entrada de Rukes.

mod logging;

fn main() -> anyhow::Result<()> {
    let _guard = logging::init()?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Rukes iniciado");
    // Fase 1: hotkey global, instancia única, ventana y motor de búsqueda.
    Ok(())
}
