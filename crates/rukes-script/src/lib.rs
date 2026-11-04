//! Descubrimiento y ejecución de scripts de usuario y motor Rhai embebido.
#![forbid(unsafe_code)]

pub mod args;
pub mod catalog;
pub mod filter;
pub mod header;
pub mod lang;
pub mod plugin;
pub mod proc;
pub mod rhai_api;
pub mod runs;
pub mod templates;

pub use catalog::{Catalog, Script};
pub use plugin::{Hooks, RunContext, Scripts, ScriptsPlugin, Settings};
pub use runs::Runs;
