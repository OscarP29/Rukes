//! Formulario del panel visual de ajustes: los ajustes más usados de `config.toml`, con sus
//! valores actuales, y la traducción de los cambios a ediciones del archivo.

use rukes_config::edit::EditValue;
use rukes_config::{Config, OnShow, WindowPosition};
use rukes_core::i18n::tr;
use rukes_ui::{SettingField, SettingKind, SettingValue, SettingsForm, SettingsSection};

fn toggle(key: &str, label: &str, help: &str, value: bool) -> SettingField {
    SettingField {
        key: key.into(),
        label: tr(label).into(),
        help: if help.is_empty() {
            String::new()
        } else {
            tr(help).into()
        },
        kind: SettingKind::Toggle,
        value: SettingValue::Bool(value),
    }
}

fn choice(
    key: &str,
    label: &str,
    help: &str,
    options: Vec<(String, String)>,
    value: &str,
) -> SettingField {
    SettingField {
        key: key.into(),
        label: tr(label).into(),
        help: if help.is_empty() {
            String::new()
        } else {
            tr(help).into()
        },
        kind: SettingKind::Choice(options),
        value: SettingValue::Text(value.into()),
    }
}

fn number(key: &str, label: &str, (min, max, step): (i64, i64, i64), value: i64) -> SettingField {
    SettingField {
        key: key.into(),
        label: tr(label).into(),
        help: String::new(),
        kind: SettingKind::Number { min, max, step },
        value: SettingValue::Int(value.clamp(min, max)),
    }
}

/// `(valor, clave de i18n)` → opciones traducidas.
fn opts(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(v, k)| ((*v).to_owned(), tr(k).to_owned()))
        .collect()
}

fn position(p: WindowPosition) -> &'static str {
    match p {
        WindowPosition::Center => "center",
        WindowPosition::TopCenter => "top_center",
        WindowPosition::BottomCenter => "bottom_center",
    }
}

fn on_show(o: OnShow) -> &'static str {
    match o {
        OnShow::Clear => "clear",
        OnShow::Keep => "keep",
        OnShow::SelectAll => "select_all",
    }
}

/// Construye el formulario. `themes`: `(nombre de archivo, nombre visible, oscuro)`.
pub fn form(c: &Config, themes: &[(String, String, bool)]) -> SettingsForm {
    let theme_opts = |filter: Option<bool>| -> Vec<(String, String)> {
        themes
            .iter()
            .filter(|(_, _, dark)| filter.is_none_or(|d| d == *dark))
            .map(|(id, name, _)| (id.clone(), name.clone()))
            .collect()
    };
    let mut main_themes = vec![(
        rukes_config::theme::SYSTEM.to_owned(),
        tr("settings.theme_system").to_owned(),
    )];
    main_themes.extend(theme_opts(None));

    let g = &c.general;
    let w = &c.window;
    let hint_options = rukes_ui::hint_options();
    let p = &c.plugins;
    let general = SettingsSection {
        title: tr("settings.tab_general").into(),
        fields: vec![
            choice(
                "general.theme",
                "settings.theme",
                "settings.theme_help",
                main_themes,
                &g.theme,
            ),
            choice(
                "general.dark_theme",
                "settings.dark_theme",
                "",
                theme_opts(Some(true)),
                &g.dark_theme,
            ),
            choice(
                "general.light_theme",
                "settings.light_theme",
                "",
                theme_opts(Some(false)),
                &g.light_theme,
            ),
            choice(
                "general.language",
                "settings.language",
                "",
                opts(&[
                    ("auto", "settings.language_auto"),
                    ("es", "settings.language_es"),
                    ("en", "settings.language_en"),
                ]),
                &g.language,
            ),
            SettingField {
                key: "general.hotkey".into(),
                label: tr("settings.hotkey").into(),
                help: tr("settings.hotkey_help").into(),
                kind: SettingKind::Text,
                value: SettingValue::Text(g.hotkey.clone()),
            },
            toggle(
                "general.start_with_windows",
                "settings.start_with_windows",
                "",
                g.start_with_windows,
            ),
            choice(
                "general.terminal",
                "settings.terminal",
                "",
                opts(&[
                    ("wt", "settings.terminal_wt"),
                    ("none", "settings.terminal_none"),
                ]),
                &g.terminal,
            ),
        ],
    };
    let window = SettingsSection {
        title: tr("settings.tab_window").into(),
        fields: vec![
            choice(
                "window.position",
                "settings.position",
                "",
                opts(&[
                    ("center", "settings.position_center"),
                    ("top_center", "settings.position_top"),
                    ("bottom_center", "settings.position_bottom"),
                ]),
                position(w.position),
            ),
            number(
                "window.width",
                "settings.width",
                (400, 1400, 20),
                i64::from(w.width),
            ),
            number(
                "window.max_results",
                "settings.max_results",
                (1, 20, 1),
                i64::try_from(w.max_results).unwrap_or(8),
            ),
            toggle(
                "window.hints",
                "settings.hints",
                "settings.hints_help",
                w.hints,
            ),
            number(
                "window.hints_count",
                "settings.hints_count",
                (1, i64::try_from(hint_options.len()).unwrap_or(16), 1),
                i64::try_from(w.hints_count).unwrap_or(8),
            ),
            SettingField {
                key: "window.hints_items".into(),
                label: tr("settings.hints_items").into(),
                help: tr("settings.hints_items_help").into(),
                value: SettingValue::List(if w.hints_items.is_empty() {
                    hint_options.iter().map(|(id, _)| id.clone()).collect()
                } else {
                    w.hints_items.clone()
                }),
                kind: SettingKind::Multi(hint_options),
            },
            choice(
                "window.on_show",
                "settings.on_show",
                "",
                opts(&[
                    ("select_all", "settings.on_show_select"),
                    ("keep", "settings.on_show_keep"),
                    ("clear", "settings.on_show_clear"),
                ]),
                on_show(w.on_show),
            ),
            choice(
                "window.backdrop",
                "settings.backdrop",
                "settings.backdrop_help",
                opts(&[
                    ("auto", "settings.backdrop_auto"),
                    ("mica", "settings.backdrop_mica"),
                    ("acrylic", "settings.backdrop_acrylic"),
                    ("solid", "settings.backdrop_solid"),
                ]),
                &w.backdrop,
            ),
            toggle(
                "window.hide_on_blur",
                "settings.hide_on_blur",
                "",
                w.hide_on_blur,
            ),
            toggle(
                "window.hide_after_action",
                "settings.hide_after_action",
                "",
                w.hide_after_action,
            ),
            toggle("window.animations", "settings.animations", "", w.animations),
            toggle(
                "window.preview_panel",
                "settings.preview_panel",
                "settings.preview_panel_help",
                w.preview_panel,
            ),
        ],
    };
    let plugins = SettingsSection {
        title: tr("settings.tab_plugins").into(),
        fields: vec![
            toggle("plugins.apps.enabled", "apps.name", "", p.apps.enabled),
            toggle("plugins.files.enabled", "files.name", "", p.files.enabled),
            toggle("plugins.calc.enabled", "calc.name", "", p.calc.enabled),
            toggle(
                "plugins.web.enabled",
                "settings.plugin_web",
                "",
                p.web.enabled,
            ),
            toggle(
                "plugins.shell.enabled",
                "settings.plugin_shell",
                "",
                p.shell.enabled,
            ),
            toggle(
                "plugins.windows.enabled",
                "windows.name",
                "",
                p.windows.enabled,
            ),
            toggle(
                "plugins.processes.enabled",
                "processes.name",
                "",
                p.processes.enabled,
            ),
            toggle(
                "plugins.controls.enabled",
                "controls.name",
                "",
                p.controls.enabled,
            ),
            toggle("plugins.http.enabled", "http.name", "", p.http.enabled),
            toggle(
                "plugins.generators.enabled",
                "generators.name",
                "",
                p.generators.enabled,
            ),
            toggle(
                "plugins.clipboard.enabled",
                "clip.name",
                "settings.plugin_clipboard_help",
                p.clipboard.enabled,
            ),
            toggle(
                "plugins.snippets.enabled",
                "snippets.name",
                "",
                p.snippets.enabled,
            ),
            toggle(
                "plugins.notes.enabled",
                "settings.plugin_notes",
                "",
                p.notes.enabled,
            ),
            toggle("plugins.wall.enabled", "wall.name", "", p.wall.enabled),
            toggle(
                "plugins.external.enabled",
                "external.name",
                "",
                p.external.enabled,
            ),
        ],
    };
    let extras = SettingsSection {
        title: tr("settings.tab_extras").into(),
        fields: vec![
            toggle(
                "plugins.calc.currency",
                "currency.name",
                "",
                p.calc.currency,
            ),
            toggle(
                "plugins.calc.currency_online",
                "settings.currency_online",
                "settings.currency_online_help",
                p.calc.currency_online,
            ),
            toggle("plugins.calc.colors", "color.name", "", p.calc.colors),
            toggle(
                "plugins.files.everything",
                "settings.everything",
                "settings.everything_help",
                p.files.everything,
            ),
            toggle(
                "plugins.apps.include_path",
                "settings.include_path",
                "settings.include_path_help",
                p.apps.include_path,
            ),
        ],
    };
    SettingsForm {
        sections: vec![general, window, plugins, extras],
    }
}

/// Cambios del panel → ediciones de `config.toml`. Valida antes lo que el archivo no puede
/// comprobar por sí solo (el atajo).
pub fn edits(changes: &[(String, SettingValue)]) -> Result<Vec<(String, EditValue)>, String> {
    changes
        .iter()
        .map(|(key, v)| {
            let value = match v {
                SettingValue::Bool(b) => EditValue::Bool(*b),
                SettingValue::Int(i) => EditValue::Int(*i),
                SettingValue::List(l) => EditValue::List(l.clone()),
                SettingValue::Text(t) => {
                    let t = t.trim();
                    if key == "general.hotkey" {
                        rukes_config::Hotkey::parse(t)
                            .map_err(|e| format!("{}: {e}", tr("settings.hotkey")))?;
                    }
                    EditValue::Text(t.to_owned())
                }
            };
            Ok((key.clone(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_reflects_the_config_and_every_key_exists() {
        let mut c = Config::default();
        c.window.position = WindowPosition::BottomCenter;
        c.window.hints_items = vec!["new".into(), "calc".into()];
        c.window.hints_count = 5;
        let themes = rukes_config::theme::available(std::path::Path::new("no-existe"));
        let f = form(&c, &themes);
        assert_eq!(f.sections.len(), 4);
        let all: Vec<&SettingField> = f.sections.iter().flat_map(|s| &s.fields).collect();
        let pos = all.iter().find(|x| x.key == "window.position").unwrap();
        assert_eq!(pos.value, SettingValue::Text("bottom_center".into()));
        // Escribir el valor actual de cada ajuste deja una configuración válida e igual.
        let changes: Vec<(String, SettingValue)> = all
            .iter()
            .map(|x| (x.key.clone(), x.value.clone()))
            .collect();
        let text = rukes_config::edit::apply("", &edits(&changes).unwrap()).unwrap();
        let back = rukes_config::parse(&text, std::path::Path::new("c.toml")).unwrap();
        assert_eq!(back.window, c.window);
        assert_eq!(back.general, c.general);
        assert_eq!(back.plugins, c.plugins);
        // Las opciones de tema incluyen «según el sistema» y Midnight.
        let theme = all.iter().find(|x| x.key == "general.theme").unwrap();
        let SettingKind::Choice(o) = &theme.kind else {
            panic!("tema sin opciones");
        };
        assert!(o.iter().any(|(v, _)| v == "system"));
        assert!(o.iter().any(|(v, _)| v == "midnight"));
        // Sin elegir ninguno, la leyenda marca todos.
        let f = form(&Config::default(), &themes);
        let items = f
            .sections
            .iter()
            .flat_map(|s| &s.fields)
            .find(|x| x.key == "window.hints_items")
            .unwrap();
        let SettingValue::List(l) = &items.value else {
            panic!("no es una lista");
        };
        assert_eq!(l.len(), rukes_ui::hint_options().len());
    }

    #[test]
    fn bad_hotkey_is_rejected_before_writing() {
        for bad in ["Ctrl+A+B", "Space", "  "] {
            let change = [("general.hotkey".to_owned(), SettingValue::Text(bad.into()))];
            assert!(edits(&change).is_err(), "{bad}");
        }
        let good = [(
            "general.hotkey".to_owned(),
            SettingValue::Text(" Ctrl+Space ".into()),
        )];
        assert_eq!(
            edits(&good).unwrap(),
            vec![(
                "general.hotkey".to_owned(),
                EditValue::Text("Ctrl+Space".into())
            )]
        );
    }
}
