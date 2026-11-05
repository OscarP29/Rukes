//! Plugins integrados de Rukes (apps, archivos, calculadora, shell...).
#![forbid(unsafe_code)]

pub mod apps;
pub mod calc;
pub mod clipboard;
pub mod color;
pub mod controls;
pub mod currency;
pub mod external;
pub mod files;
pub mod generators;
pub mod notes;
pub mod processes;
pub mod shell;
pub mod snippets;
pub mod util;
pub mod wall;
pub mod web;
pub mod windows;
pub mod workflows;

pub use apps::AppsPlugin;
pub use calc::CalcPlugin;
pub use clipboard::{ClipHistory, ClipSettings, ClipboardPlugin};
pub use color::ColorPlugin;
pub use controls::ControlsPlugin;
pub use currency::CurrencyPlugin;
pub use external::{ExternalPlugin, ExternalPlugins};
pub use files::{FileIndex, FilesPlugin, FilesSettings};
pub use generators::GeneratorsPlugin;
pub use notes::{NotesPlugin, RemindPlugin, RemindStore, TodoPlugin};
pub use processes::ProcessesPlugin;
pub use shell::{ShellPlugin, ShellSettings, SystemCommandsPlugin};
pub use snippets::{SnippetStore, SnippetsPlugin};
pub use wall::{WallPlugin, WallSettings};
pub use web::WebPlugin;
pub use windows::WindowsPlugin;
pub use workflows::{Runner, Scheduler, WorkflowStore, WorkflowsPlugin};
