//! Traducción de `config.toml` a los ajustes de cada componente.

use rukes_config::{Config, OnShow, WindowPosition};
use rukes_core::EngineConfig;
use rukes_platform::dwm::Backdrop;
use rukes_platform::window::Placement;
use rukes_plugins::apps::AppsSettings;
use rukes_ui::UiConfig;

pub fn engine(c: &Config) -> EngineConfig {
    EngineConfig {
        frecency_weight: c.search.frecency_weight.clamp(0.0, 1.0),
        ..EngineConfig::default()
    }
}

pub fn ui(c: &Config) -> UiConfig {
    UiConfig {
        width: c.window.width.clamp(360, 1600) as f32,
        max_results: c.window.max_results.clamp(1, 20),
        hide_on_blur: c.window.hide_on_blur,
        hide_after_action: c.window.hide_after_action,
        on_show: match c.window.on_show {
            OnShow::Clear => rukes_ui::OnShow::Clear,
            OnShow::Keep => rukes_ui::OnShow::Keep,
            OnShow::SelectAll => rukes_ui::OnShow::SelectAll,
        },
    }
}

pub fn placement(c: &Config) -> Placement {
    match c.window.position {
        WindowPosition::Center => Placement::CENTER,
        WindowPosition::TopCenter => Placement::TOP_CENTER,
    }
}

pub fn backdrop(c: &Config) -> Backdrop {
    Backdrop::from_config(&c.window.backdrop)
}

pub fn apps(c: &Config) -> AppsSettings {
    AppsSettings {
        aliases: c.plugins.apps.aliases.clone(),
        include_path: c.plugins.apps.include_path,
        priority: c.plugins.apps.priority.clamp(-1.0, 1.0),
    }
}

/// Alto lógico de la ventana con la lista llena; fija la posición vertical (RF-002).
pub fn reference_height(c: &Config) -> f32 {
    let rows = c.window.max_results.clamp(1, 20) as f32;
    56.0 + 1.0 + rows * 48.0 + 12.0
}
