//! `new` / `nuevo` / `crear`: crea temas, workflows, scripts y plugins desde el launcher. Pide
//! lo que falte (qué crear, el nombre y, si hace falta, el lenguaje o el tema base), escribe el
//! archivo en su carpeta y lo abre en el editor.

use std::io;
use std::path::{Path, PathBuf};

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata,
    Query, ResultItem,
};
use rukes_script::templates::{default_alias, slug};
use serde_json::{Value, json};

/// Qué se puede crear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Theme,
    Workflow,
    Script,
    Plugin,
}

impl Kind {
    const ALL: [Self; 4] = [Self::Theme, Self::Workflow, Self::Script, Self::Plugin];

    /// Palabras que lo nombran tras `new` (la primera es la que se escribe al elegirlo).
    fn words(self) -> &'static [&'static str] {
        match self {
            Self::Theme => &["theme", "tema"],
            Self::Workflow => &["workflow", "flujo"],
            Self::Script => &["script"],
            Self::Plugin => &["plugin"],
        }
    }

    fn id(self) -> &'static str {
        self.words()[0]
    }

    /// Palabra que se escribe al elegirlo: en español si la palabra clave lo es.
    fn word(self, es: bool) -> &'static str {
        if es && self == Self::Theme {
            "tema"
        } else {
            self.id()
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Workflow => "workflow",
            Self::Script => "script",
            Self::Plugin => "plugin",
        }
    }

    fn from_word(w: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.words().iter().any(|x| x.eq_ignore_ascii_case(w)))
    }
}

/// Lenguaje de un plugin externo nuevo: `(id, nombre, archivo, programa, argumentos, plantilla)`.
struct PluginLang {
    id: &'static str,
    label: &'static str,
    main: &'static str,
    executable: &'static str,
    args: &'static [&'static str],
    body: &'static str,
}

const PLUGIN_LANGS: &[PluginLang] = &[
    PluginLang {
        id: "python",
        label: "Python",
        main: "main.py",
        executable: "python",
        args: &["-u", "main.py"],
        body: include_str!("../templates/plugin.py"),
    },
    PluginLang {
        id: "node",
        label: "Node.js",
        main: "main.js",
        executable: "node",
        args: &["main.js"],
        body: include_str!("../templates/plugin.js"),
    },
    PluginLang {
        id: "powershell",
        label: "PowerShell",
        main: "main.ps1",
        executable: "powershell",
        args: &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "main.ps1",
        ],
        body: include_str!("../templates/plugin.ps1"),
    },
];

/// Plugin `create`.
pub struct CreatePlugin {
    /// Carpeta de temas del usuario.
    pub themes_dir: PathBuf,
    /// Carpeta de workflows (`None`: los workflows están desactivados).
    pub workflows_dir: Option<PathBuf>,
    /// Carpeta de plugins externos (`None`: desactivados).
    pub plugins_dir: Option<PathBuf>,
    /// Los scripts están activados.
    pub scripts: bool,
    /// `config.toml`, para activar un tema nuevo.
    pub config_file: PathBuf,
    /// Editor de `[scripts] editor`.
    pub editor: String,
    /// Tema en uso (base por defecto de uno nuevo).
    pub current_theme: String,
}

/// Cadena TOML entre comillas.
fn toml_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `base`, `base-2`, `base-3`... el primero para el que `taken` es falso.
fn free_name(base: &str, taken: impl Fn(&str) -> bool) -> io::Result<String> {
    (1..1000)
        .map(|n| {
            if n == 1 {
                base.to_owned()
            } else {
                format!("{base}-{n}")
            }
        })
        .find(|c| !taken(c))
        .ok_or_else(|| io::Error::other("no hay un nombre libre"))
}

/// Nombre limpio: sin espacios de más ni saltos de línea.
fn clean_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Id de archivo a partir del nombre (`Mi tema` → `mi-tema`).
fn file_id(name: &str, fallback: &str) -> String {
    let s = slug(name);
    if s == "nuevo-script" && clean_name(name).is_empty() {
        fallback.to_owned()
    } else {
        s
    }
}

/// Crea el archivo de un tema que hereda de `base`, con todos los valores de la base comentados
/// para cambiarlos fácilmente. Devuelve `(id del tema, ruta)`.
pub fn create_theme(dir: &Path, name: &str, base: &str) -> Result<(String, PathBuf), String> {
    let t = rukes_config::theme::load(base, dir).map_err(|e| e.to_string())?;
    let builtin: Vec<&str> = rukes_config::theme::BUILTIN
        .iter()
        .map(|(k, _)| *k)
        .collect();
    // Un nombre de tema incluido lo sustituiría (y heredaría de sí mismo).
    let id = free_name(&file_id(name, "mi-tema"), |c| {
        builtin.contains(&c)
            || c == rukes_config::theme::SYSTEM
            || dir.join(format!("{c}.toml")).exists()
    })
    .map_err(|e| e.to_string())?;
    let name = clean_name(name);
    let c = &t.colors;
    let l = &t.layout;
    let text = format!(
        "# Tema «{name}» creado desde Rukes. Hereda todo de «{base}»: quita el # de lo que quieras\n\
         # cambiar y guarda; Rukes lo aplica al momento. Colores: #rrggbb o #rrggbbaa.\n\
         [theme]\n\
         name = {qname}\n\
         extends = {qbase}\n\
         # dark = {dark}\n\
         # backdrop = {backdrop}      # mica, acrylic o solid\n\
         # opacity = {opacity}           # opacidad del fondo sobre Mica/Acrílico\n\
         # radius = {radius}\n\
         \n\
         [theme.colors]\n\
         # background = \"{bg}\"\n\
         # foreground = \"{fg}\"\n\
         # muted = \"{muted}\"\n\
         # accent = \"{accent}\"\n\
         # selection = \"{sel}\"\n\
         # selection_foreground = \"{selfg}\"\n\
         # divider = \"{div}\"         # también «border»\n\
         {tokens}\
         \n\
         [theme.font]\n\
         # family = {family}\n\
         # size = {size}\n\
         # mono = {mono}\n\
         \n\
         [theme.layout]\n\
         # row_height = {row}\n\
         # icon_size = {icon}\n\
         # search_height = {search}\n\
         # padding = {pad}\n\
         # spacing = {spacing}\n",
        qname = toml_str(&name),
        qbase = toml_str(base),
        dark = t.dark,
        backdrop = toml_str(&t.backdrop),
        opacity = t.opacity,
        radius = t.radius,
        bg = c.background,
        fg = c.foreground,
        muted = c.muted,
        accent = c.accent,
        sel = c.selection,
        selfg = c.selection_foreground,
        div = c.divider,
        tokens = [
            ("surface", c.surface),
            ("surface_hover", c.surface_hover),
            ("text_faint", c.text_faint),
            ("accent_soft", c.accent_soft),
        ]
        .iter()
        .map(|(k, v)| match v {
            Some(v) => format!("# {k} = \"{v}\"\n"),
            None => format!("# {k}: derivado de los demás colores\n"),
        })
        .collect::<String>(),
        family = toml_str(&t.font.family),
        size = t.font.size,
        mono = toml_str(&t.font.mono),
        row = l.row_height,
        icon = l.icon_size,
        search = l.search_height,
        pad = l.padding,
        spacing = l.spacing,
    );
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{id}.toml"));
    write_new(&path, &text).map_err(|e| e.to_string())?;
    Ok((id, path))
}

/// Crea un archivo de workflows con uno de ejemplo. Devuelve `(id, alias, ruta)`.
pub fn create_workflow(dir: &Path, name: &str) -> io::Result<(String, String, PathBuf)> {
    let id = free_name(&file_id(name, "mi-workflow"), |c| {
        dir.join(format!("{c}.toml")).exists()
    })?;
    let name = clean_name(name);
    let alias = default_alias(&name);
    let text = format!(
        "# Workflow «{name}» creado desde Rukes. Todos los pasos posibles están explicados en\n\
         # ejemplos.toml, en esta misma carpeta. Al guardar se recarga solo: escribe «{alias}» en\n\
         # Rukes para ejecutarlo (Ctrl+P muestra los pasos).\n\
         \n\
         [workflow.{id}]\n\
         name = {qname}\n\
         alias = [{qalias}]\n\
         # hotkey = \"Ctrl+Alt+1\"\n\
         steps = [\n\
         \x20 {{ notify = {qstart} }},\n\
         \x20 # {{ app = \"notepad\" }},\n\
         \x20 # {{ url = \"https://example.com\", window = {{ snap = \"right\" }} }},\n\
         \x20 # {{ wait = \"1s\" }},\n\
         \x20 # {{ script = \"nombre de un script\" }},\n\
         ]\n\
         \n\
         # Para ejecutarlo solo a una hora o cada cierto tiempo:\n\
         # [[trigger]]\n\
         # at = \"09:00\"              # o every = \"30m\", o on = \"login\"\n\
         # days = [\"laborables\"]\n\
         # run = \"workflow:{id}\"\n",
        qname = toml_str(&name),
        qalias = toml_str(&alias),
        qstart = toml_str(&format!("«{name}» en marcha")),
    );
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{id}.toml"));
    write_new(&path, &text)?;
    Ok((id, alias, path))
}

/// Crea la carpeta de un plugin externo con su `plugin.toml` y su programa. Devuelve
/// `(palabra clave, archivo principal)`.
fn create_plugin(root: &Path, name: &str, lang: &PluginLang) -> io::Result<(String, PathBuf)> {
    let id = free_name(&file_id(name, "mi-plugin"), |c| root.join(c).exists())?;
    let name = clean_name(name);
    let dir = root.join(&id);
    std::fs::create_dir_all(&dir)?;
    let args = lang
        .args
        .iter()
        .map(|a| toml_str(a))
        .collect::<Vec<_>>()
        .join(", ");
    let manifest = format!(
        "# Plugin «{name}» creado desde Rukes (protocolo: docs/plugin-protocol.md).\n\
         # Al guardar este archivo o {main}, Rukes lo recarga solo. Escribe «plugins» en Rukes para\n\
         # ver su estado y sus errores.\n\
         \n\
         [plugin]\n\
         name = {qname}\n\
         version = \"0.1.0\"\n\
         description = {qdesc}\n\
         executable = {exe}\n\
         args = [{args}]\n\
         keywords = [{kw}]       # «{id} hola» le envía «hola»\n\
         # prefix = \"%\"\n\
         global = false\n\
         icon = \"plugin\"\n\
         \n\
         # Informativos: se muestran al instalarlo y en «plugins».\n\
         [permissions]\n\
         clipboard = true\n\
         network = false\n\
         filesystem = false\n\
         \n\
         # Llega al plugin en «initialize» (params.config).\n\
         [config]\n\
         \n\
         # Lo que muestra «help {id}».\n\
         [help]\n\
         summary = {qdesc}\n\
         \n\
         [[help.usage]]\n\
         example = {qexample}\n\
         description = \"Texto en mayúsculas\"\n",
        main = lang.main,
        qname = toml_str(&name),
        qdesc = toml_str(&format!("Plugin «{name}»")),
        exe = toml_str(lang.executable),
        kw = toml_str(&id),
        qexample = toml_str(&format!("{id} hola")),
    );
    write_new(&dir.join(rukes_plugins::external::MANIFEST), &manifest)?;
    let mut body = lang.body.replace("{{name}}", &name);
    // Windows PowerShell 5.1 lee los .ps1 sin BOM como ANSI.
    if lang.main.ends_with(".ps1") {
        body.insert(0, '\u{feff}');
    }
    let main = dir.join(lang.main);
    write_new(&main, &body)?;
    Ok((id, main))
}

/// Escribe un archivo nuevo; falla si ya existe.
fn write_new(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(text.as_bytes())
}

fn failed(e: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(e.to_string())
}

impl CreatePlugin {
    fn enabled(&self, k: Kind) -> bool {
        match k {
            Kind::Theme => true,
            Kind::Workflow => self.workflows_dir.is_some(),
            Kind::Script => self.scripts,
            Kind::Plugin => self.plugins_dir.is_some(),
        }
    }

    /// La palabra clave usada está en español.
    fn spanish(kw: &str) -> bool {
        ["nuevo", "nueva", "crear"].contains(&kw.to_lowercase().as_str())
    }

    /// `new script` en el idioma de la palabra clave usada (lo atiende el plugin de scripts).
    fn script_query(kw: &str, name: &str) -> String {
        let base = if Self::spanish(kw) {
            "nuevo script"
        } else {
            "new script"
        };
        format!("{base} {name}")
    }

    fn kind_items(&self, kw: &str, text: &str) -> Vec<ResultItem> {
        let mut f = Fuzzy::new(text);
        let mut out = Vec::new();
        for (n, k) in Kind::ALL
            .into_iter()
            .filter(|k| self.enabled(*k))
            .enumerate()
        {
            let title = tr(&format!("create.kind_{}", k.id())).to_owned();
            let score = if text.is_empty() {
                1.0 - n as f32 * 0.01
            } else {
                match k
                    .words()
                    .iter()
                    .chain([&title.as_str()])
                    .filter_map(|w| f.score(w))
                    .reduce(f32::max)
                {
                    Some(s) => s,
                    None => continue,
                }
            };
            let mut it = ResultItem::new(format!("kind:{}", k.id()), title);
            it.subtitle = tr(&format!("create.kind_{}_sub", k.id())).into();
            it.icon = Icon::Named(k.icon().into());
            it.fuzzy = score;
            it.alias_match = true;
            let next = if k == Kind::Script {
                Self::script_query(kw, "")
            } else {
                format!("{kw} {} ", k.word(Self::spanish(kw)))
            };
            it.payload = json!({ "k": "query", "q": next }).to_string();
            it.actions = vec![Action::new(ActionId::OPEN, tr("create.choose"))];
            out.push(it);
        }
        out
    }

    /// Fila que pide el nombre.
    fn ask_name(kw: &str, k: Kind) -> ResultItem {
        let mut it = ResultItem::new(
            format!("ask:{}", k.id()),
            tr(&format!("create.ask_{}", k.id())),
        );
        it.subtitle = trf(
            "create.ask_sub",
            &[("example", &format!("{kw} {} …", k.word(Self::spanish(kw))))],
        );
        it.icon = Icon::Named(k.icon().into());
        it.fuzzy = 1.0;
        it.alias_match = true;
        it.payload = json!({ "k": "wait" }).to_string();
        it.actions = vec![Action::new(ActionId::OPEN, tr("create.type_name"))];
        it
    }

    fn theme_items(&self, name: &str) -> Vec<ResultItem> {
        let themes = rukes_config::theme::available(&self.themes_dir);
        let current = rukes_config::theme::normalize(&self.current_theme);
        let mut bases: Vec<&(String, String, bool)> =
            themes.iter().filter(|t| t.0 == current).collect();
        bases.extend(themes.iter().filter(|t| t.0 != current));
        let file = format!("{}.toml", file_id(name, "mi-tema"));
        bases
            .into_iter()
            .enumerate()
            .map(|(n, (id, shown, _))| {
                let mut it = ResultItem::new(
                    format!("theme:{id}"),
                    trf(
                        "create.theme_from",
                        &[("name", &clean_name(name)), ("base", shown)],
                    ),
                );
                it.subtitle = if n == 0 {
                    trf("create.theme_sub_current", &[("file", &file)])
                } else {
                    trf("create.theme_sub", &[("file", &file)])
                };
                it.icon = Icon::Named("theme".into());
                it.fuzzy = 1.0 - n as f32 * 0.01;
                it.alias_match = true;
                it.payload = json!({ "k": "theme", "name": name, "base": id }).to_string();
                it.actions = vec![Action::new(ActionId::OPEN, tr("create.create_use"))];
                it
            })
            .collect()
    }

    fn workflow_item(name: &str) -> ResultItem {
        let mut it = ResultItem::new(
            "workflow:new",
            trf("create.workflow_new", &[("name", &clean_name(name))]),
        );
        it.subtitle = trf(
            "create.opens",
            &[("file", &format!("{}.toml", file_id(name, "mi-workflow")))],
        );
        it.icon = Icon::Named("workflow".into());
        it.fuzzy = 1.0;
        it.alias_match = true;
        it.payload = json!({ "k": "workflow", "name": name }).to_string();
        it.actions = vec![Action::new(ActionId::OPEN, tr("create.create"))];
        it
    }

    fn plugin_items(name: &str) -> Vec<ResultItem> {
        let id = file_id(name, "mi-plugin");
        PLUGIN_LANGS
            .iter()
            .enumerate()
            .map(|(n, lang)| {
                let mut it = ResultItem::new(
                    format!("plugin:{}", lang.id),
                    trf(
                        "create.plugin_new",
                        &[("name", &clean_name(name)), ("lang", &lang.label)],
                    ),
                );
                it.subtitle = trf(
                    "create.plugin_sub",
                    &[("file", &format!("{id}/{}", lang.main)), ("keyword", &id)],
                );
                it.icon = Icon::Named("plugin".into());
                it.fuzzy = 1.0 - n as f32 * 0.01;
                it.alias_match = true;
                it.payload = json!({ "k": "plugin", "name": name, "lang": lang.id }).to_string();
                it.actions = vec![Action::new(ActionId::OPEN, tr("create.create"))];
                it
            })
            .collect()
    }

    fn open(&self, path: &Path) -> Result<(), PluginError> {
        rukes_script::plugin::open_in_editor(&self.editor, path).map_err(failed)
    }
}

impl Plugin for CreatePlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "create".into(),
            name: tr("create.name").into(),
            prefix: None,
            keywords: ["new", "nuevo", "nueva", "crear", "create"]
                .map(String::from)
                .to_vec(),
            priority: 0.3,
            global: false,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let kw = q
            .raw
            .split_whitespace()
            .next()
            .unwrap_or("new")
            .to_lowercase();
        let text = q.text.trim();
        let (first, rest) = text
            .split_once(char::is_whitespace)
            .map_or((text, ""), |(a, b)| (a, b.trim()));
        let kind = Kind::from_word(first).filter(|k| self.enabled(*k));
        let out = match kind {
            None => self.kind_items(&kw, text),
            Some(k) if rest.is_empty() && k != Kind::Script => vec![Self::ask_name(&kw, k)],
            Some(Kind::Theme) => self.theme_items(rest),
            Some(Kind::Workflow) => vec![Self::workflow_item(rest)],
            Some(Kind::Plugin) => Self::plugin_items(rest),
            Some(Kind::Script) => {
                // Lo crea el plugin de scripts, que ya pide el lenguaje.
                let mut it = ResultItem::new("kind:script", tr("create.kind_script"));
                it.subtitle = tr("create.kind_script_sub").into();
                it.icon = Icon::Named("script".into());
                it.fuzzy = 1.0;
                it.alias_match = true;
                it.payload =
                    json!({ "k": "query", "q": Self::script_query(&kw, rest) }).to_string();
                it.actions = vec![Action::new(ActionId::OPEN, tr("create.choose"))];
                vec![it]
            }
        };
        sink(out);
    }

    fn execute(&self, item: &ResultItem, _action: &str) -> Result<ActionOutcome, PluginError> {
        let p: Value = serde_json::from_str(&item.payload).map_err(failed)?;
        let s = |k: &str| p[k].as_str().unwrap_or_default().to_owned();
        match s("k").as_str() {
            "query" => Ok(ActionOutcome::ReplaceQuery(s("q"))),
            "wait" => Ok(ActionOutcome::KeepOpen),
            "theme" => {
                let (id, path) =
                    create_theme(&self.themes_dir, &s("name"), &s("base")).map_err(failed)?;
                rukes_config::edit::save(
                    &self.config_file,
                    &[(
                        "general.theme".into(),
                        rukes_config::edit::EditValue::Text(id),
                    )],
                )
                .map_err(failed)?;
                self.open(&path)?;
                Ok(ActionOutcome::Hide)
            }
            "workflow" => {
                let dir = self
                    .workflows_dir
                    .as_ref()
                    .ok_or_else(|| failed("workflows"))?;
                let (_, _, path) = create_workflow(dir, &s("name")).map_err(failed)?;
                self.open(&path)?;
                Ok(ActionOutcome::Hide)
            }
            "plugin" => {
                let root = self.plugins_dir.as_ref().ok_or_else(|| failed("plugins"))?;
                let lang = PLUGIN_LANGS
                    .iter()
                    .find(|l| l.id == s("lang"))
                    .ok_or_else(|| failed("lenguaje desconocido"))?;
                let (_, main) = create_plugin(root, &s("name"), lang).map_err(failed)?;
                self.open(&main)?;
                Ok(ActionOutcome::Hide)
            }
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::process::{Command, Stdio};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rukes-create-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn plugin(dir: &Path) -> CreatePlugin {
        CreatePlugin {
            themes_dir: dir.join("themes"),
            workflows_dir: Some(dir.join("workflows")),
            plugins_dir: Some(dir.join("plugins")),
            scripts: true,
            config_file: dir.join("config.toml"),
            editor: String::new(),
            current_theme: "nord".into(),
        }
    }

    fn query(p: &CreatePlugin, raw: &str) -> Vec<ResultItem> {
        let kw = raw.split_whitespace().next().unwrap();
        let q = Query {
            id: rukes_core::QueryId(1),
            raw: raw.into(),
            text: raw[kw.len()..].trim().into(),
            targeted: true,
        };
        let mut got = Vec::new();
        p.query(&q, &CancelToken::default(), &mut |v| got = v);
        got
    }

    #[test]
    fn asks_what_then_the_name_then_the_language() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = tmp("flow");
        let p = plugin(&dir);
        // Qué crear.
        let kinds = query(&p, "nuevo ");
        assert_eq!(kinds.len(), 4);
        assert!(matches!(
            p.execute(&kinds[0], ActionId::OPEN).unwrap(),
            ActionOutcome::ReplaceQuery(q) if q == "nuevo tema "
        ));
        let script = kinds.iter().find(|i| i.id == "kind:script").unwrap();
        assert!(matches!(
            p.execute(script, ActionId::OPEN).unwrap(),
            ActionOutcome::ReplaceQuery(q) if q == "nuevo script "
        ));
        assert_eq!(query(&p, "new plu")[0].id, "kind:plugin");
        // El nombre.
        let ask = query(&p, "new tema ");
        assert_eq!(ask[0].id, "ask:theme");
        assert!(matches!(
            p.execute(&ask[0], ActionId::OPEN).unwrap(),
            ActionOutcome::KeepOpen
        ));
        // El tema base: primero el actual.
        let themes = query(&p, "new theme Mi tema");
        assert_eq!(themes[0].id, "theme:nord");
        assert!(themes.len() > 5);
        // El lenguaje del plugin.
        let langs = query(&p, "crear plugin Mi bot");
        assert_eq!(langs.len(), PLUGIN_LANGS.len());
        assert!(langs[0].subtitle.contains("mi-bot"));
        assert!(matches!(
            p.execute(&query(&p, "new script Backup")[0], ActionId::OPEN).unwrap(),
            ActionOutcome::ReplaceQuery(q) if q == "new script Backup"
        ));
        // Desactivados: no se ofrecen.
        let off = CreatePlugin {
            workflows_dir: None,
            plugins_dir: None,
            scripts: false,
            ..plugin(&dir)
        };
        assert_eq!(query(&off, "new ").len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn new_theme_extends_the_base_and_loads() {
        let dir = tmp("theme");
        let themes = dir.join("themes");
        let (id, path) = create_theme(&themes, "Mi \"tema\"", "nord").unwrap();
        assert_eq!(id, "mi-tema");
        let t = rukes_config::theme::load(&id, &themes).unwrap();
        let nord = rukes_config::theme::load("nord", &themes).unwrap();
        assert_eq!(t.name, "Mi \"tema\"");
        assert_eq!(t.colors, nord.colors);
        // Quitar los # aplica los valores de la base tal cual.
        let text: String = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| match l.strip_prefix("# ") {
                Some(v) if v.contains(" = ") => format!("{v}\n"),
                _ => format!("{l}\n"),
            })
            .collect();
        std::fs::write(&path, text).unwrap();
        let t = rukes_config::theme::load(&id, &themes).unwrap();
        assert_eq!(t.colors, nord.colors);
        assert_eq!(t.layout, nord.layout);
        // Un nombre de tema incluido no lo sustituye; uno repetido busca otro nombre.
        assert_eq!(create_theme(&themes, "Nord", "nord").unwrap().0, "nord-2");
        assert_eq!(
            create_theme(&themes, "mi tema", "nord").unwrap().0,
            "mi-tema-2"
        );
        assert!(create_theme(&themes, "x", "no-existe").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn new_workflow_is_valid() {
        let dir = tmp("wf");
        let (id, alias, path) = create_workflow(&dir, "Modo foco").unwrap();
        assert_eq!((id.as_str(), alias.as_str()), ("modo-foco", "modo foco"));
        let text = std::fs::read_to_string(&path).unwrap();
        let (w, t, e) = rukes_plugins::workflows::parse_file(&text, &path);
        assert!(e.is_empty(), "{e:?}");
        assert!(t.is_empty());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].alias, ["modo foco"]);
        assert_eq!(create_workflow(&dir, "Modo foco").unwrap().0, "modo-foco-2");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Arranca el plugin como lo haría Rukes y comprueba las tres llamadas.
    fn talk(dir: &Path, lang: &PluginLang) {
        let m = rukes_plugins::external::Manifest::load(dir).unwrap();
        assert_eq!(m.executable, lang.executable);
        let Some(exe) = rukes_script::lang::find_in_path(&m.executable) else {
            eprintln!("{} no está instalado: se omite", m.executable);
            return;
        };
        let mut child = Command::new(exe)
            .args(&m.args)
            .current_dir(dir)
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for line in [
            r#"{"id":1,"method":"initialize","params":{"protocol":1,"config":{}}}"#,
            r#"{"method":"cancel","params":{"query_id":1}}"#,
            r#"{"id":2,"method":"query","params":{"query_id":2,"text":"año","raw":"x año","targeted":true}}"#,
            r#"{"id":3,"method":"action","params":{"item":{"id":"mayusculas","data":"AÑO"},"action":"open"}}"#,
        ] {
            writeln!(stdin, "{line}").unwrap();
        }
        drop(stdin);
        let out: Vec<Value> = BufReader::new(child.stdout.take().unwrap())
            .lines()
            .map(|l| serde_json::from_str(&l.unwrap()).unwrap())
            .collect();
        assert!(child.wait().unwrap().success(), "{}", lang.id);
        assert_eq!(out.len(), 3, "{}: {out:?}", lang.id);
        assert_eq!(out[0]["id"], 1);
        assert_eq!(out[1]["result"][0]["title"], "AÑO", "{}: {out:?}", lang.id);
        assert_eq!(out[2]["result"], json!({ "copy": "AÑO" }), "{}", lang.id);
    }

    #[test]
    fn new_plugins_answer_the_protocol() {
        let root = tmp("plugins");
        for lang in PLUGIN_LANGS {
            let (kw, main) = create_plugin(&root, &format!("Mi {}", lang.id), lang).unwrap();
            assert_eq!(kw, format!("mi-{}", lang.id));
            let m = rukes_plugins::external::Manifest::load(main.parent().unwrap()).unwrap();
            assert_eq!(m.keywords, std::slice::from_ref(&kw));
            assert_eq!(m.id, format!("ext:{kw}"));
            talk(main.parent().unwrap(), lang);
        }
        // Una carpeta repetida busca otro nombre.
        let (kw, _) = create_plugin(&root, "Mi python", &PLUGIN_LANGS[0]).unwrap();
        assert_eq!(kw, "mi-python-2");
        let (found, errors) = rukes_plugins::external::discover(&root);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(found.len(), PLUGIN_LANGS.len() + 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
