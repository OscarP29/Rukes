//! Plugins integrados de Rukes (apps, archivos, calculadora, shell...).
#![forbid(unsafe_code)]

pub mod apps;
pub mod calc;
pub mod files;
pub mod generators;
pub mod processes;
pub mod shell;
pub mod util;
pub mod web;
pub mod windows;

pub use apps::AppsPlugin;
pub use calc::CalcPlugin;
pub use files::{FileIndex, FilesPlugin, FilesSettings};
pub use generators::GeneratorsPlugin;
pub use processes::ProcessesPlugin;
pub use shell::{ShellPlugin, ShellSettings, SystemCommandsPlugin};
pub use web::WebPlugin;
pub use windows::WindowsPlugin;
