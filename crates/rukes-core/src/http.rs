//! Datos del mini Postman que comparten el plugin `http`, la aplicación y la UI.

use serde::{Deserialize, Serialize};

/// Métodos del selector, en orden.
pub const HTTP_METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

/// Petición tal como se edita: cada pestaña es texto plano.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpForm {
    /// Método en mayúsculas (`GET`...). Vacío: `GET`, o `POST` si hay cuerpo.
    pub method: String,
    /// Dirección tal como se escribió (`3000/api`, `https://...`).
    pub url: String,
    /// Cabeceras, una por línea: `Nombre: valor`.
    pub headers: String,
    /// Cuerpo.
    pub body: String,
    /// Autenticación: `Bearer <token>`, `Basic usuario:clave`, `usuario:clave` o un token.
    pub auth: String,
    /// Parámetros de la query, uno por línea: `clave=valor`.
    pub params: String,
}

/// Respuesta lista para mostrar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// Código de estado.
    pub status: u32,
    /// Texto del estado (`OK`, `Not Found`...).
    pub reason: String,
    /// Tiempo total en milisegundos.
    pub ms: u128,
    /// Bytes recibidos del cuerpo.
    pub size: usize,
    /// El cuerpo se cortó por tamaño.
    pub truncated: bool,
    /// Cabeceras, una por línea.
    pub headers: String,
    /// Cuerpo legible: JSON con sangrado, texto o un aviso si es binario.
    pub body: String,
    /// `Content-Type` en minúsculas (vacío si no hay).
    pub content_type: String,
    /// La petición enviada como orden de `curl`.
    pub curl: String,
}
