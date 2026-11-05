//! Traducción de `config.toml` a los ajustes de cada componente.

use rukes_config::{Config, OnShow, WindowPosition};
use rukes_core::EngineConfig;
use rukes_platform::window::Placement;
use rukes_plugins::apps::AppsSettings;
use rukes_ui::{UiConfig, UiStyle};

pub fn engine(c: &Config) -> EngineConfig {
    EngineConfig {
        frecency_weight: c.search.frecency_weight.clamp(0.0, 1.0),
        ..EngineConfig::default()
    }
}

/// `animations`: las de Windows; `scale`: escalado de texto del sistema (RF-214).
pub fn ui(c: &Config, style: UiStyle, animations: bool, scale: f32) -> UiConfig {
    UiConfig {
        width: (c.window.width.clamp(360, 1600) as f32 * scale).round(),
        max_results: c.window.max_results.clamp(1, 20),
        hide_on_blur: c.window.hide_on_blur,
        hide_after_action: c.window.hide_after_action,
        on_show: match c.window.on_show {
            OnShow::Clear => rukes_ui::OnShow::Clear,
            OnShow::Keep => rukes_ui::OnShow::Keep,
            OnShow::SelectAll => rukes_ui::OnShow::SelectAll,
        },
        style,
        animations: c.window.animations && animations,
        preview_panel: c.window.preview_panel,
        preview_width: (c.window.preview_width.clamp(200, 900) as f32 * scale).round(),
        anchor_bottom: c.window.position == WindowPosition::BottomCenter,
        hints: c.window.hints,
        hints_count: c.window.hints_count,
        hints_items: c.window.hints_items.clone(),
    }
}

pub fn placement(c: &Config) -> Placement {
    match c.window.position {
        WindowPosition::Center => Placement::CENTER,
        WindowPosition::TopCenter => Placement::TOP_CENTER,
        WindowPosition::BottomCenter => Placement::BOTTOM_CENTER,
    }
}

pub fn apps(c: &Config) -> AppsSettings {
    AppsSettings {
        aliases: c.plugins.apps.aliases.clone(),
        include_path: c.plugins.apps.include_path,
        profiles: c
            .plugins
            .apps
            .profiles
            .iter()
            .filter(|p| !p.name.trim().is_empty() && !p.target.trim().is_empty())
            .map(|p| rukes_plugins::apps::Profile {
                name: p.name.trim().to_owned(),
                target: rukes_script::args::expand_path(p.target.trim())
                    .to_string_lossy()
                    .into_owned(),
                args: p.args.clone(),
                cwd: (!p.cwd.trim().is_empty())
                    .then(|| rukes_script::args::expand_path(p.cwd.trim())),
                alias: p.alias.iter().map(|a| a.trim().to_lowercase()).collect(),
                admin: p.admin,
            })
            .collect(),
        priority: c.plugins.apps.priority.clamp(-1.0, 1.0),
    }
}

/// Alto lógico de la ventana con la lista llena; fija la posición vertical (RF-002).
pub fn reference_height(c: &Config, s: &UiStyle) -> f32 {
    let rows = c.window.max_results.clamp(1, 20) as f32;
    s.search_height + 1.0 + rows * s.row_height + 12.0
}

pub fn files(c: &Config) -> rukes_plugins::FilesSettings {
    let f = &c.plugins.files;
    rukes_plugins::FilesSettings {
        roots: f
            .roots
            .iter()
            .map(|r| rukes_plugins::files::expand_root(r))
            .collect(),
        exclude: f.exclude.clone(),
        include_hidden: f.include_hidden,
        min_chars: f.min_chars,
        priority: f.priority,
        shell: c.plugins.shell.shell.clone(),
        terminal: c.general.terminal.clone(),
        everything: f.everything,
    }
}

/// Carpeta de scripts: la configurada o `scripts` junto a la configuración (RF-130).
pub fn scripts_dir(c: &Config, paths: &rukes_config::paths::Paths) -> std::path::PathBuf {
    let d = c.scripts.dir.trim();
    if d.is_empty() {
        paths.config_dir.join("scripts")
    } else {
        rukes_script::args::expand_path(d)
    }
}

/// Ruta configurada (admite `~` y `%VAR%`) o `default` si está vacía.
fn path_or(s: &str, default: std::path::PathBuf) -> std::path::PathBuf {
    let s = s.trim();
    if s.is_empty() {
        default
    } else {
        rukes_script::args::expand_path(s)
    }
}

pub fn clipboard(c: &Config) -> rukes_plugins::ClipSettings {
    let s = &c.plugins.clipboard;
    rukes_plugins::ClipSettings {
        max_items: s.max_items,
        expire_days: s.expire_days,
        exclude: s.exclude.iter().map(|e| e.trim().to_lowercase()).collect(),
        paste: s.paste,
    }
}

/// Carpeta de snippets (RF-110).
pub fn snippets_dir(c: &Config, paths: &rukes_config::paths::Paths) -> std::path::PathBuf {
    path_or(&c.plugins.snippets.dir, paths.snippets_dir())
}

/// Carpeta de plugins externos (RF-170).
pub fn plugins_dir(c: &Config, paths: &rukes_config::paths::Paths) -> std::path::PathBuf {
    path_or(&c.plugins.external.dir, paths.plugins_dir())
}

/// Fondos de pantalla (RF-180).
pub fn wall(c: &Config) -> rukes_plugins::WallSettings {
    let w = &c.plugins.wall;
    rukes_plugins::WallSettings {
        keyword: w.keyword.trim().to_lowercase(),
        dirs: w
            .dirs
            .iter()
            .filter(|d| !d.trim().is_empty())
            .map(|d| rukes_script::args::expand_path(d.trim()))
            .collect(),
        position: rukes_platform::wallpaper::Position::parse(&w.position).unwrap_or_default(),
        interval: rukes_plugins::workflows::parse_duration(&w.interval),
        shuffle: w.shuffle,
    }
}

/// Carpeta de workflows (sección 5.14).
pub fn workflows_dir(c: &Config, paths: &rukes_config::paths::Paths) -> std::path::PathBuf {
    path_or(&c.workflows.dir, paths.workflows_dir())
}

/// Archivos de notas y de tareas (RF-120, RF-121).
pub fn notes_files(
    c: &Config,
    paths: &rukes_config::paths::Paths,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let n = &c.plugins.notes;
    (
        path_or(&n.notes_file, paths.notes_dir().join("notas.md")),
        path_or(&n.todo_file, paths.notes_dir().join("tareas.md")),
    )
}

pub fn scripts(c: &Config) -> rukes_script::Settings {
    let s = &c.scripts;
    rukes_script::Settings {
        editor: s.editor.clone(),
        default_timeout: rukes_script::header::parse_duration(&s.default_timeout)
            .unwrap_or(std::time::Duration::from_secs(30)),
        interpreters: s
            .interpreters
            .iter()
            .map(|(k, v)| (k.trim_start_matches('.').to_ascii_lowercase(), v.clone()))
            .collect(),
        priority: s.priority,
    }
}
