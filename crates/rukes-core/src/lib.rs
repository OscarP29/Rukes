//! Motor de Rukes: búsqueda, ranking, trait [`Plugin`] y acciones.
//!
//! Este crate es lógica pura: no conoce Windows ni la UI.
#![forbid(unsafe_code)]

mod engine;
pub mod frecency;
pub mod fuzzy;
mod help;
pub mod i18n;
mod item;
pub mod persist;
mod plugin;
mod query;
pub mod ranking;

pub use engine::{Engine, EngineConfig, QueryBatch};
pub use frecency::Frecency;
pub use fuzzy::Fuzzy;
pub use help::{HelpUsage, PluginHelp};
pub use item::{Action, ActionId, Icon, ResultItem};
pub use plugin::{ActionOutcome, Plugin, PluginCommand, PluginError, PluginMetadata};
pub use query::{CancelToken, Query, QueryId};
