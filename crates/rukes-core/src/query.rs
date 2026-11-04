//! Consultas y cancelación.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Identificador incremental de una consulta. Una consulta con id mayor invalida las anteriores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct QueryId(pub u64);

/// Token de cancelación compartido entre el motor y los plugins.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// Crea un token no cancelado.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marca la consulta como obsoleta.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// `true` si la consulta ya no interesa. Los plugins deben comprobarlo en bucles largos.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Consulta tal y como la recibe un plugin.
#[derive(Debug, Clone)]
pub struct Query {
    /// Identificador de la consulta.
    pub id: QueryId,
    /// Texto completo escrito por el usuario.
    pub raw: String,
    /// Texto para el plugin, sin prefijo ni palabra clave y sin espacios sobrantes.
    pub text: String,
    /// `true` si el usuario restringió la búsqueda a este plugin (prefijo o palabra clave).
    pub targeted: bool,
}

/// Normaliza una consulta para usarla como clave (frecency, historial).
pub fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}
