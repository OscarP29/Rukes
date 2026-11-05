//! Plugins externos: procesos aparte que hablan JSON por stdin/stdout (RF-170..RF-173).
//!
//! El protocolo está en `docs/plugin-protocol.md`.

mod manifest;
mod plugin;
mod process;

pub use manifest::{MANIFEST, Manifest, discover};
pub use plugin::{
    ACTION_TIMEOUT, ExternalPlugin, ExternalPlugins, PluginsListPlugin, QUERY_TIMEOUT,
};
pub use process::{CallError, INIT_TIMEOUT, MAX_FAILURES, PROTOCOL, PluginProcess};

#[cfg(test)]
mod tests;
