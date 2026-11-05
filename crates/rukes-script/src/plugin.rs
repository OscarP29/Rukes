//! Plugin `scripts`: los scripts del usuario en el launcher (sección 5.13).
//!
//! - `alias argumentos` ejecuta el script con esos argumentos (`gh issue Mi bug`).
//! - `new script [nombre]` crea uno desde una plantilla y lo abre en el editor.
//! - `logs [texto]` muestra las ejecuciones (en curso y terminadas).

use std::collections::BTreeMap;
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata,
    Query, ResultItem,
};
use rukes_plugins::util::{ellipsis, fold, strip_any};
use serde_json::{Value, json};

use crate::args::{self, expand_path};
use crate::catalog::{Catalog, Script};
use crate::filter::{self, FilterItem, ItemAction};
use crate::header::OutputMode;
use crate::lang::{self, Lang};
use crate::proc::{self, Exit};
use crate::rhai_api::{self, Notifier};
use crate::runs::{Record, Running, Runs, Status};
use crate::templates::{self, TEMPLATES};

/// Id del plugin.
pub const ID: &str = "scripts";

const NEW_PHRASES: [&str; 2] = ["new script", "nuevo script"];
const LOGS_PHRASES: [&str; 1] = ["logs"];
/// Espera antes de lanzar un *script filter* (las pulsaciones seguidas lo cancelan).
const FILTER_DEBOUNCE: Duration = Duration::from_millis(150);
/// Tiempo máximo de un *script filter*.
const FILTER_TIMEOUT: Duration = Duration::from_secs(5);
/// Líneas de salida que se muestran en el launcher.
const MAX_LINES: usize = 200;

/// Ajustes del plugin.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Editor (comando o ruta).
    pub editor: String,
    /// Tiempo máximo por defecto.
    pub default_timeout: Duration,
    /// Extensión → intérprete.
    pub interpreters: BTreeMap<String, String>,
    /// Prioridad.
    pub priority: f32,
}

/// Contexto del usuario al ejecutar (RF-141).
#[derive(Debug, Clone, Default)]
pub struct RunContext {
    /// Título de la ventana activa antes de abrir el launcher.
    pub active_window: String,
    /// Ejecutable de esa ventana.
    pub active_exe: String,
    /// Archivos seleccionados en el Explorador, si la ventana activa era una.
    pub selected: Vec<PathBuf>,
}

/// Lo que el plugin necesita de la aplicación.
#[derive(Clone)]
pub struct Hooks {
    /// Notificación del sistema.
    pub notify: Notifier,
    /// Sustituye en la UI el resultado provisional con clave `scripts:run:<id>`.
    pub results: Arc<dyn Fn(String, Vec<ResultItem>) + Send + Sync>,
    /// Contexto de la ventana activa.
    pub context: Arc<dyn Fn() -> RunContext + Send + Sync>,
}

/// Plugin de scripts.
pub struct ScriptsPlugin {
    catalog: Arc<Catalog>,
    runs: Arc<Runs>,
    settings: Settings,
    hooks: Hooks,
}

fn payload(v: &Value) -> String {
    v.to_string()
}

fn item(id: String, title: impl Into<String>, data: &Value) -> ResultItem {
    let mut r = ResultItem::new(id, title);
    r.plugin_id = ID.into();
    r.payload = payload(data);
    r
}

fn duration_text(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0).replace('.', ",")
    } else {
        format!("{} min {} s", ms / 60_000, (ms / 1000) % 60)
    }
}

fn ago(unix: u64) -> String {
    let s = crate::runs::now_unix().saturating_sub(unix);
    let (n, unit) = if s < 60 {
        (s, "s")
    } else if s < 3600 {
        (s / 60, "min")
    } else if s < 86_400 {
        (s / 3600, "h")
    } else {
        (s / 86_400, "d")
    };
    trf("scripts.ago", &[("n", &n), ("unit", &unit)])
}

fn status_text(r: &Record) -> String {
    let d = duration_text(r.duration_ms);
    match r.status {
        Status::Ok => format!("✓ {d}"),
        Status::Failed => format!(
            "✗ {} {} · {d}",
            tr("scripts.exit_code"),
            r.code.unwrap_or(-1)
        ),
        Status::Timeout => format!("⏱ {} · {d}", tr("scripts.timed_out")),
        Status::Cancelled => format!("■ {} · {d}", tr("scripts.cancelled")),
        Status::Error => format!("✗ {}", tr("scripts.could_not_start")),
        Status::Detached => tr("scripts.detached").into(),
    }
}

fn last_line(s: &str) -> Option<&str> {
    s.lines().map(str::trim).rfind(|l| !l.is_empty())
}

fn expand_cwd(script: &Script) -> PathBuf {
    match &script.meta.cwd {
        Some(c) => {
            let p = expand_path(c);
            if p.is_absolute() {
                p
            } else {
                script.dir().join(p)
            }
        }
        None => script.dir().to_path_buf(),
    }
}

/// Abre `path` con el editor configurado o, si no existe, con el Bloc de notas.
pub fn open_in_editor(editor: &str, path: &Path) -> Result<(), String> {
    let mut words = lang::split_command(editor);
    if !words.is_empty()
        && let Some(program) = lang::find_in_path(&words.remove(0))
    {
        let spawned = Command::new(program)
            .args(words)
            .arg(path)
            .current_dir(path.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(proc::CREATE_NO_WINDOW)
            .spawn();
        if spawned.is_ok() {
            return Ok(());
        }
    }
    rukes_platform::shell::launch(
        "notepad.exe",
        Some(&proc::join_args(&[path.to_string_lossy()])),
        None,
        rukes_platform::shell::Verb::Open,
    )
    .map_err(|e| e.to_string())
}

/// Variables de entorno de una ejecución (RF-141).
fn environment(
    script: &Script,
    scripts_dir: &Path,
    query: &str,
    values: &[(String, String)],
    ctx: &RunContext,
    clipboard_chars: usize,
) -> Vec<(String, String)> {
    let mut env = vec![
        ("RUKES_QUERY".to_owned(), query.to_owned()),
        (
            "RUKES_SCRIPT".into(),
            script.path.to_string_lossy().into_owned(),
        ),
        ("RUKES_SCRIPT_NAME".into(), script.meta.name.clone()),
        (
            "RUKES_SCRIPTS_DIR".into(),
            scripts_dir.to_string_lossy().into_owned(),
        ),
        ("RUKES_ACTIVE_WINDOW".into(), ctx.active_window.clone()),
        ("RUKES_ACTIVE_PROCESS".into(), ctx.active_exe.clone()),
        (
            "RUKES_SELECTED_FILE".into(),
            ctx.selected
                .first()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        ),
        (
            "RUKES_SELECTED_FILES".into(),
            ctx.selected
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        (
            "RUKES_CLIPBOARD".into(),
            rukes_platform::clipboard::get_text(clipboard_chars).unwrap_or_default(),
        ),
        ("RUKES_VERSION".into(), env!("CARGO_PKG_VERSION").into()),
        ("PYTHONUTF8".into(), "1".into()),
        ("PYTHONIOENCODING".into(), "utf-8".into()),
    ];
    for (k, v) in values {
        let name: String = k
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect();
        env.push((format!("RUKES_ARG_{name}"), v.clone()));
    }
    env
}

/// Resultado de una ejecución, antes de pasar al historial.
struct Outcome {
    exit: Exit,
    stdout: String,
    stderr: String,
    items: Option<Vec<FilterItem>>,
    detached: bool,
}

impl Outcome {
    fn error(msg: String) -> Self {
        Self {
            exit: Exit::SpawnError(msg.clone()),
            stdout: String::new(),
            stderr: msg,
            items: None,
            detached: false,
        }
    }
}

impl ScriptsPlugin {
    /// Crea el plugin.
    pub fn new(catalog: Arc<Catalog>, runs: Arc<Runs>, settings: Settings, hooks: Hooks) -> Self {
        Self {
            catalog,
            runs,
            settings,
            hooks,
        }
    }

    fn script_item(
        &self,
        s: &Script,
        input: &str,
        alias: Option<&str>,
        fuzzy: f32,
    ) -> Vec<ResultItem> {
        let path = s.path.to_string_lossy().into_owned();
        let parsed = args::parse(&s.meta.args, input);
        let data = |inp: &str| json!({"k": "script", "p": path, "i": inp, "a": alias});
        let icon = s
            .meta
            .icon
            .as_ref()
            .map(|i| s.dir().join(expand_path(i)))
            .filter(|p| p.is_file())
            .map_or_else(
                || {
                    if s.lang == Lang::Exe {
                        Icon::File(s.path.clone())
                    } else {
                        Icon::Named("script".into())
                    }
                },
                Icon::File,
            );
        let actions = vec![
            Action::new(ActionId::OPEN, tr("scripts.run")),
            Action::new(ActionId::RUN_AS_ADMIN, "action.run_as_admin"),
            Action::new(ActionId::EDIT, tr("scripts.edit")),
            Action::new(ActionId::DELETE, tr("scripts.move_to_recycle_bin")),
            Action::new(ActionId::OPEN_LOCATION, "action.open_location"),
            Action::new(ActionId::COPY_PATH, "action.copy_path"),
        ];
        let mut out = Vec::new();
        let typed = alias.is_some() && !input.trim().is_empty();
        let subtitle = if let Some(e) = parsed.error.as_ref().filter(|_| typed) {
            format!("⚠ {e}")
        } else if !parsed.missing.is_empty() && alias.is_some() {
            let missing: Vec<String> = s
                .meta
                .args
                .iter()
                .filter(|a| parsed.missing.contains(&a.name))
                .map(|a| format!("{} ({})", a.name, a.kind.label()))
                .collect();
            format!("{}: {}", tr("scripts.type"), missing.join(", "))
        } else if typed {
            parsed
                .values
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| format!("{k} = {}", ellipsis(v, 40)))
                .collect::<Vec<_>>()
                .join(" · ")
        } else if !s.meta.description.is_empty() {
            s.meta.description.clone()
        } else {
            s.meta
                .aliases
                .first()
                .map(|a| format!("{} «{a}»", tr("scripts.alias")))
                .unwrap_or_else(|| s.path.to_string_lossy().into_owned())
        };
        if let Some((idx, options)) = &parsed.choices
            && alias.is_some()
        {
            for (n, o) in options.iter().enumerate() {
                let completed = args::complete(&s.meta.args, input, *idx, o);
                let mut it = item(
                    format!("s:{path}#{idx}:{o}"),
                    format!("{}: {o}", s.meta.name),
                    &data(&completed),
                );
                it.subtitle = s.meta.args[*idx].name.clone();
                it.icon = icon.clone();
                it.fuzzy = fuzzy - 0.001 * (n + 1) as f32;
                it.alias_match = true;
                it.actions = actions.clone();
                out.push(it);
            }
        }
        let mut it = item(format!("s:{path}"), s.meta.name.clone(), &data(input));
        it.subtitle = subtitle;
        it.icon = icon;
        it.fuzzy = fuzzy;
        it.alias_match = alias.is_some();
        it.actions = actions;
        it.preview = Some(s.path.clone());
        out.insert(0, it);
        out
    }

    fn template_items(&self, name: &str) -> Vec<ResultItem> {
        let shown = if name.is_empty() {
            tr("scripts.new_script").to_owned()
        } else {
            name.to_owned()
        };
        TEMPLATES
            .iter()
            .enumerate()
            .map(|(n, tpl)| {
                let mut it = item(
                    format!("new:{}", tpl.ext),
                    format!(
                        "{}: {shown} ({})",
                        tr("scripts.create_script"),
                        tr(tpl.label)
                    ),
                    &json!({"k": "new", "ext": tpl.ext, "name": name}),
                );
                it.subtitle = format!(
                    "{}.{} · {}",
                    templates::slug(&shown),
                    tpl.ext,
                    tr("scripts.opens_in_the_editor")
                );
                it.icon = Icon::Named("new".into());
                it.fuzzy = 1.0 - n as f32 * 0.01;
                it.alias_match = true;
                it.actions = vec![Action::new(ActionId::OPEN, tr("scripts.create"))];
                it
            })
            .collect()
    }

    fn log_items(&self, filter_text: &str) -> Vec<ResultItem> {
        let mut f = Fuzzy::new(filter_text);
        let mut out = Vec::new();
        let keep = |name: &str, f: &mut Fuzzy| filter_text.is_empty() || f.score(name).is_some();
        for (n, r) in self.runs.running().iter().enumerate() {
            if !keep(&r.name, &mut f) {
                continue;
            }
            let mut it = item(
                format!("run:{}", r.id),
                r.name.clone(),
                &json!({"k": "cancel", "id": r.id}),
            );
            it.subtitle = format!(
                "{} ({}) · {}",
                tr("scripts.running_ellipsis"),
                duration_text(r.started.elapsed().as_millis() as u64),
                tr("scripts.enter_cancels")
            );
            it.icon = Icon::Named("running".into());
            it.fuzzy = 1.0 - n as f32 * 0.001;
            it.alias_match = true;
            it.actions = vec![Action::new(ActionId::OPEN, tr("common.cancel"))];
            out.push(it);
        }
        let base = out.len();
        for (n, r) in self.runs.history().iter().enumerate() {
            if !keep(&r.name, &mut f) {
                continue;
            }
            let mut it = item(
                format!("log:{}", r.id),
                r.name.clone(),
                &json!({"k": "log", "id": r.id}),
            );
            let first = last_line(&r.stdout)
                .or_else(|| last_line(&r.stderr))
                .map(|l| format!(" · {}", ellipsis(l, 50)))
                .unwrap_or_default();
            it.subtitle = format!("{} · {}{first}", status_text(r), ago(r.started));
            it.icon = Icon::Named(
                if r.status == Status::Ok {
                    "ok"
                } else {
                    "error"
                }
                .into(),
            );
            it.fuzzy = 0.99 - (base + n) as f32 * 0.001;
            it.alias_match = true;
            it.actions = vec![
                Action::new(ActionId::OPEN, tr("scripts.show_output")),
                Action::new(ActionId::COPY_PATH, tr("scripts.copy_output")),
                Action::new(ActionId::EDIT, tr("scripts.edit_script")),
            ];
            out.push(it);
        }
        if out.is_empty() {
            let mut it = item(
                "log:none".into(),
                tr("scripts.no_runs_yet"),
                &json!({"k": "none"}),
            );
            it.icon = Icon::Named("log".into());
            it.alias_match = true;
            it.fuzzy = 1.0;
            out.push(it);
        }
        out
    }

    /// Convierte resultados JSON en elementos del launcher.
    fn filter_result_items(items: &[FilterItem], prefix: &str) -> Vec<ResultItem> {
        items
            .iter()
            .take(MAX_LINES)
            .enumerate()
            .map(|(n, fi)| {
                let (k, v) = match fi.action() {
                    ItemAction::Open(o) => ("open", o),
                    ItemAction::Copy(c) => ("copy", c),
                    ItemAction::Query(q) => ("query", q),
                };
                let mut it = item(
                    format!("{prefix}{n}"),
                    fi.title.clone(),
                    &json!({"k": k, "v": v}),
                );
                it.subtitle = fi.subtitle.clone();
                it.icon = fi
                    .icon
                    .as_ref()
                    .map(PathBuf::from)
                    .filter(|p| p.is_file())
                    .map_or(Icon::Named("script".into()), Icon::File);
                it.fuzzy = 1.0 - n as f32 * 0.0001;
                it.alias_match = true;
                it.actions = vec![Action::new(
                    ActionId::OPEN,
                    match k {
                        "open" => "action.open",
                        "copy" => "action.copy_path",
                        _ => tr("scripts.search"),
                    },
                )];
                it
            })
            .collect()
    }

    /// Lista que se muestra al terminar una ejecución en modo launcher o al abrir un log.
    fn output_items(r: &Record, items: Option<&[FilterItem]>) -> Vec<ResultItem> {
        if let Some(items) = items.filter(|_| r.status == Status::Ok) {
            let mut v = Self::filter_result_items(items, &format!("out:{}:", r.id));
            if v.is_empty() {
                let mut it = item(
                    format!("out:{}:empty", r.id),
                    r.name.clone(),
                    &json!({"k": "none"}),
                );
                it.subtitle = tr("search.no_results").into();
                v.push(it);
            }
            return v;
        }
        let mut head = item(
            format!("out:{}:head", r.id),
            format!("{} · {}", r.name, status_text(r)),
            &json!({"k": "copy", "v": r.output()}),
        );
        head.subtitle = tr("scripts.enter_copies_output").into();
        head.icon = Icon::Named(
            if r.status == Status::Ok {
                "ok"
            } else {
                "error"
            }
            .into(),
        );
        head.actions = vec![Action::new(ActionId::OPEN, "action.copy_path")];
        let mut out = vec![head];
        let lines = r
            .stdout
            .lines()
            .map(|l| (l, false))
            .chain(r.stderr.lines().map(|l| (l, true)))
            .filter(|(l, _)| !l.trim().is_empty())
            .take(MAX_LINES);
        for (n, (line, err)) in lines.enumerate() {
            let mut it = item(
                format!("out:{}:{n}", r.id),
                ellipsis(line.trim_end(), 200),
                &json!({"k": "copy", "v": line.trim_end()}),
            );
            it.subtitle = if err { "stderr" } else { "" }.into();
            it.actions = vec![Action::new(ActionId::OPEN, "action.copy_path")];
            out.push(it);
        }
        for (n, it) in out.iter_mut().enumerate() {
            it.score = 1.0 - n as f32 * 0.0001;
        }
        out
    }

    /// Ejecuta el script (bloqueante). Lo llaman el hilo de la ejecución y los filtros.
    #[allow(clippy::too_many_arguments)]
    fn run_script(
        &self,
        script: &Script,
        values: &[(String, String)],
        query: &str,
        mode: OutputMode,
        admin: bool,
        deadline: Option<Instant>,
        cancel: rhai_api::Cancel,
        job: Arc<proc::JobSlot>,
    ) -> Outcome {
        let ctx = (self.hooks.context)();
        let cwd = expand_cwd(script);
        if !cwd.is_dir() {
            return Outcome::error(format!("{}: {}", tr("scripts.no_workdir"), cwd.display()));
        }
        let user_args: Vec<String> = values.iter().map(|(_, v)| v.clone()).collect();
        if script.lang == Lang::Rhai {
            let source = match std::fs::read_to_string(&script.path) {
                Ok(s) => s,
                Err(e) => return Outcome::error(e.to_string()),
            };
            let env = environment(script, self.catalog.dir(), query, values, &ctx, 1 << 20);
            let o = rhai_api::run(&rhai_api::Input {
                source: source.trim_start_matches('\u{feff}'),
                name: &script.meta.name,
                query,
                args: values,
                env: &env,
                deadline,
                cancel,
                notify: Arc::clone(&self.hooks.notify),
                job,
            });
            return Outcome {
                exit: o.exit,
                stdout: o.stdout,
                stderr: o.stderr,
                items: o.items,
                detached: false,
            };
        }
        let interactive = mode == OutputMode::Terminal;
        let launch = match lang::launch(
            &script.path,
            &script.lang,
            script.meta.interpreter.as_deref(),
            &self.settings.interpreters,
            interactive || admin,
        ) {
            Ok(l) => l,
            Err(e) => return Outcome::error(e),
        };
        let pause = tr("scripts.press_enter_to_close");
        if admin {
            // La elevación no hereda el entorno: el script envoltorio fija las variables.
            let env = environment(script, self.catalog.dir(), query, values, &ctx, 2000);
            let wrapper =
                proc::wrapper_script(&launch, &user_args, &env, interactive.then_some(pause));
            let params = format!(
                "-NoLogo -NoProfile -ExecutionPolicy Bypass -EncodedCommand {}",
                proc::encode_ps(&wrapper)
            );
            let shell = lang::powershell();
            return match rukes_platform::shell::launch_with(
                &shell.to_string_lossy(),
                Some(&params),
                Some(&cwd),
                rukes_platform::shell::Verb::RunAs,
                interactive,
            ) {
                Ok(()) => Outcome {
                    exit: Exit::Code(0),
                    stdout: String::new(),
                    stderr: String::new(),
                    items: None,
                    detached: true,
                },
                Err(e) => Outcome::error(e.to_string()),
            };
        }
        let env = environment(script, self.catalog.dir(), query, values, &ctx, 16 * 1024);
        if interactive {
            let wrapper = proc::wrapper_script(&launch, &user_args, &[], Some(pause));
            let mut cmd = Command::new(lang::powershell());
            cmd.args([
                "-NoLogo",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-EncodedCommand",
            ])
            .arg(proc::encode_ps(&wrapper))
            .envs(env)
            .current_dir(&cwd)
            .creation_flags(proc::CREATE_NEW_CONSOLE);
            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => return Outcome::error(e.to_string()),
            };
            if let Ok(j) = rukes_platform::job::Job::new() {
                let _ = j.assign(&child);
                *job.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(j));
            }
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            *job.lock().unwrap_or_else(|p| p.into_inner()) = None;
            return Outcome {
                exit: if cancel() {
                    Exit::Cancelled
                } else {
                    Exit::Code(code)
                },
                stdout: tr("scripts.output_shown_in_the_terminal").into(),
                stderr: String::new(),
                items: None,
                detached: false,
            };
        }
        let mut cmd = Command::new(&launch.program);
        cmd.args(&launch.args)
            .args(&user_args)
            .envs(env)
            .current_dir(&cwd);
        let c = proc::run_captured(cmd, deadline, &|| cancel(), Some(&job));
        let items = if mode == OutputMode::Launcher || script.meta.filter {
            filter::parse(&c.stdout)
        } else {
            None
        };
        Outcome {
            exit: c.exit,
            stdout: c.stdout,
            stderr: c.stderr,
            items,
            detached: false,
        }
    }

    /// Aplica el modo de salida al terminar.
    fn deliver(
        &self,
        script: &Script,
        mode: OutputMode,
        record: &Record,
        items: Option<&[FilterItem]>,
    ) {
        let name = &script.meta.name;
        let ok = record.status == Status::Ok || record.status == Status::Detached;
        let failure_text = || {
            let detail = last_line(&record.stderr)
                .or_else(|| last_line(&record.stdout))
                .map(|l| format!(" · {}", ellipsis(l, 180)))
                .unwrap_or_default();
            format!("{}{detail}", status_text(record))
        };
        match mode {
            OutputMode::Launcher => {
                (self.hooks.results)(
                    format!("{ID}:run:{}", record.id),
                    Self::output_items(record, items),
                );
            }
            OutputMode::Terminal => {}
            _ if !ok => (self.hooks.notify)(name, &failure_text(), true),
            OutputMode::Silent => {}
            OutputMode::Notification => {
                if record.status != Status::Detached {
                    let body = last_line(&record.stdout)
                        .map(|l| ellipsis(l, 200))
                        .unwrap_or_else(|| status_text(record));
                    (self.hooks.notify)(name, &body, false);
                }
            }
            OutputMode::Clipboard => {
                let text = record.stdout.trim_end_matches(['\r', '\n']);
                match rukes_platform::clipboard::set_text(text) {
                    Ok(()) => (self.hooks.notify)(
                        name,
                        &format!("{} · {}", tr("notify.copied"), ellipsis(text, 80)),
                        false,
                    ),
                    Err(e) => (self.hooks.notify)(name, &e.to_string(), true),
                }
            }
        }
    }

    /// Lanza una ejecución en su hilo. Devuelve lo que debe hacer la ventana.
    fn start(
        self: &Arc<Self>,
        script: Arc<Script>,
        values: Vec<(String, String)>,
        query: String,
        force_admin: bool,
    ) -> ActionOutcome {
        let admin = (force_admin || script.meta.admin) && script.lang != Lang::Rhai;
        let mut mode = script.meta.output;
        if mode == OutputMode::Terminal && script.lang == Lang::Rhai {
            mode = OutputMode::Launcher;
        }
        let run: Arc<Running> = self.runs.start(&script.meta.name, &script.path);
        let deadline = if mode == OutputMode::Terminal {
            None
        } else {
            script
                .timeout(self.settings.default_timeout)
                .map(|t| Instant::now() + t)
        };
        let me = Arc::clone(self);
        let r2 = Arc::clone(&run);
        let s2 = Arc::clone(&script);
        let spawned = std::thread::Builder::new()
            .name("rukes-script-run".into())
            .spawn(move || {
                let flag = r2.cancel_flag();
                let cancel: rhai_api::Cancel =
                    Arc::new(move || flag.load(std::sync::atomic::Ordering::SeqCst));
                let o = me.run_script(&s2, &values, &query, mode, admin, deadline, cancel, Arc::clone(&r2.job));
                let (status, code) = match &o.exit {
                    _ if o.detached => (Status::Detached, None),
                    Exit::Code(0) => (Status::Ok, Some(0)),
                    Exit::Code(c) => (Status::Failed, Some(*c)),
                    Exit::Timeout => (Status::Timeout, None),
                    Exit::Cancelled => (Status::Cancelled, None),
                    Exit::SpawnError(_) => (Status::Error, None),
                };
                let record = Record {
                    id: r2.id,
                    name: s2.meta.name.clone(),
                    path: s2.path.clone(),
                    started: r2.started_unix,
                    duration_ms: r2.started.elapsed().as_millis() as u64,
                    status,
                    code,
                    args: values.clone(),
                    stdout: o.stdout,
                    stderr: o.stderr,
                };
                tracing::info!(script = %record.name, status = ?record.status, ms = record.duration_ms, "script terminado");
                me.deliver(&s2, mode, &record, o.items.as_deref());
                me.runs.finish(record);
            });
        if let Err(e) = spawned {
            return ActionOutcome::Notify(e.to_string());
        }
        if mode == OutputMode::Launcher {
            let mut it = item(
                format!("run:{}", run.id),
                format!("{}: {}", tr("scripts.running"), script.meta.name),
                &json!({"k": "cancel", "id": run.id}),
            );
            it.subtitle = tr("scripts.enter_cancels_esc").into();
            it.icon = Icon::Named("running".into());
            it.actions = vec![Action::new(ActionId::OPEN, tr("common.cancel"))];
            ActionOutcome::ShowResults(vec![it])
        } else {
            ActionOutcome::Hide
        }
    }

    /// Busca un script por nombre, alias o nombre de archivo (sin distinguir mayúsculas).
    pub fn find(&self, name: &str) -> Option<Arc<Script>> {
        let n = fold(name.trim());
        self.catalog.scripts().iter().find_map(|s| {
            let file = s
                .path
                .file_name()
                .map(|f| fold(&f.to_string_lossy()))
                .unwrap_or_default();
            let stem = s
                .path
                .file_stem()
                .map(|f| fold(&f.to_string_lossy()))
                .unwrap_or_default();
            (fold(&s.meta.name) == n
                || s.meta.aliases.iter().any(|a| fold(a) == n)
                || file == n
                || stem == n)
                .then(|| Arc::clone(s))
        })
    }

    /// Ejecuta un script en segundo plano desde un workflow, un trigger o un atajo (RF-150,
    /// RF-160, RF-162). Los argumentos se interpretan como si se escribieran tras el alias.
    pub fn run_named(self: &Arc<Self>, name: &str, args: &[String]) -> Result<(), String> {
        let script = self
            .find(name)
            .ok_or_else(|| trf("scripts.not_found", &[("name", &name)]))?;
        let line = args
            .iter()
            .map(|a| {
                if a.contains(char::is_whitespace) {
                    format!("\"{a}\"")
                } else {
                    a.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let parsed = crate::args::parse(&script.meta.args, &line);
        if let Some(e) = parsed.error {
            return Err(e);
        }
        if !parsed.missing.is_empty() {
            return Err(trf(
                "scripts.missing_args",
                &[("args", &parsed.missing.join(", "))],
            ));
        }
        let query = format!("{} {line}", script.meta.name);
        self.start(script, parsed.values, query.trim().to_owned(), false);
        Ok(())
    }

    /// Atajos globales declarados en las cabeceras: `(atajo, nombre del script)`.
    pub fn hotkeys(&self) -> Vec<(String, String)> {
        self.catalog
            .scripts()
            .iter()
            .filter_map(|s| {
                s.meta
                    .hotkey
                    .as_ref()
                    .map(|h| (h.clone(), s.meta.name.clone()))
            })
            .collect()
    }

    fn run_filter(&self, s: &Script, input: &str, cancel: &CancelToken) -> Vec<ResultItem> {
        let end = Instant::now() + FILTER_DEBOUNCE;
        while Instant::now() < end {
            if cancel.is_cancelled() {
                return Vec::new();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let parsed = args::parse(&s.meta.args, input);
        let token = cancel.clone();
        let timeout = s
            .timeout(FILTER_TIMEOUT)
            .unwrap_or(FILTER_TIMEOUT)
            .min(FILTER_TIMEOUT);
        let o = self.run_script(
            s,
            &parsed.values,
            input,
            OutputMode::Launcher,
            false,
            Some(Instant::now() + timeout),
            Arc::new(move || token.is_cancelled()),
            Arc::new(proc::JobSlot::default()),
        );
        if cancel.is_cancelled() {
            return Vec::new();
        }
        match (o.exit, o.items) {
            (Exit::Code(0), Some(items)) => {
                Self::filter_result_items(&items, &format!("f:{}:", s.path.display()))
            }
            (exit, _) => {
                let mut it = item(
                    format!("f:{}:err", s.path.display()),
                    s.meta.name.clone(),
                    &json!({"k": "none"}),
                );
                it.subtitle = match exit {
                    Exit::Code(0) => tr("scripts.not_json").into(),
                    Exit::Timeout => tr("scripts.timed_out_title").into(),
                    _ => last_line(&o.stderr)
                        .map_or_else(|| format!("{exit:?}"), |l| ellipsis(l, 120)),
                };
                it.icon = Icon::Named("error".into());
                it.alias_match = true;
                it.fuzzy = 1.0;
                vec![it]
            }
        }
    }

    fn find_alias<'a>(s: &'a Script, raw: &'a str) -> Option<(&'a str, &'a str)> {
        s.meta
            .aliases
            .iter()
            .filter_map(|a| strip_any(raw, &[fold(a).as_str()]).map(|rest| (a.as_str(), rest)))
            .max_by_key(|(a, _)| a.len())
    }
}

/// Envoltorio para poder usar `Arc<Self>` en `execute` (los hilos de ejecución lo necesitan).
pub struct Scripts(pub Arc<ScriptsPlugin>);

impl Plugin for Scripts {
    fn metadata(&self) -> PluginMetadata {
        let mut keywords: Vec<String> = NEW_PHRASES
            .iter()
            .chain(LOGS_PHRASES.iter())
            .map(|s| (*s).to_owned())
            .collect();
        for s in self.0.catalog.scripts().iter() {
            keywords.extend(s.meta.aliases.iter().cloned());
        }
        PluginMetadata {
            id: ID.into(),
            name: "Scripts".into(),
            prefix: None,
            keywords,
            priority: self.0.settings.priority,
            global: true,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let p = &self.0;
        let raw = q.raw.trim();
        if let Some(rest) = strip_any(raw, &NEW_PHRASES) {
            sink(p.template_items(rest));
            return;
        }
        if let Some(rest) = strip_any(raw, &LOGS_PHRASES) {
            sink(p.log_items(rest));
            return;
        }
        let mut out = Vec::new();
        let mut f = Fuzzy::new(&q.text);
        for s in p.catalog.scripts().iter() {
            if cancel.is_cancelled() {
                return;
            }
            if let Some((alias, rest)) = ScriptsPlugin::find_alias(s, raw) {
                if s.meta.filter {
                    sink(out);
                    sink(p.run_filter(s, rest, cancel));
                    return;
                }
                out.extend(p.script_item(s, rest, Some(alias), 1.0));
            } else if !q.targeted {
                let score = std::iter::once(s.meta.name.as_str())
                    .chain(s.meta.aliases.iter().map(String::as_str))
                    .filter_map(|n| f.score(n))
                    .fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.max(v))));
                if let Some(score) = score {
                    out.extend(p.script_item(s, "", None, score));
                }
            }
        }
        if !q.targeted
            && q.text.chars().count() >= 3
            && let Some(score) = [tr("scripts.new_script"), "new script"]
                .iter()
                .filter_map(|n| f.score(n))
                .reduce(f32::max)
        {
            let mut it = item(
                "new".into(),
                tr("scripts.new_script_ellipsis"),
                &json!({"k": "query", "v": format!("{} ", NEW_PHRASES[0])}),
            );
            it.subtitle = tr("scripts.new_script_sub").into();
            it.icon = Icon::Named("new".into());
            it.fuzzy = score * 0.9;
            out.push(it);
        }
        sink(out);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let p = &self.0;
        let data: Value = serde_json::from_str(&item.payload).unwrap_or(Value::Null);
        let k = data["k"].as_str().unwrap_or("");
        let str_of = |key: &str| data[key].as_str().unwrap_or("").to_owned();
        let failed = |e: String| PluginError::Failed(e);
        match k {
            "script" => {
                let path = PathBuf::from(str_of("p"));
                match action {
                    ActionId::EDIT => {
                        return open_in_editor(&p.settings.editor, &path)
                            .map(|()| ActionOutcome::Hide)
                            .map_err(failed);
                    }
                    ActionId::OPEN_LOCATION => {
                        return rukes_platform::shell::open_location(&path)
                            .map(|()| ActionOutcome::Hide)
                            .map_err(|e| failed(e.to_string()));
                    }
                    ActionId::COPY_PATH => {
                        return rukes_plugins::util::copy(&path.to_string_lossy());
                    }
                    ActionId::DELETE => {
                        let mut yes = ResultItem::new(
                            "confirm:delete",
                            format!(
                                "{}: {}",
                                tr("scripts.yes_move_to_recycle_bin"),
                                path.display()
                            ),
                        );
                        yes.payload = payload(&json!({"k": "delete", "p": path}));
                        yes.icon = Icon::Named("error".into());
                        let mut no = ResultItem::new("cancel", tr("common.cancel"));
                        no.payload = payload(&json!({"k": "query", "v": ""}));
                        return Ok(ActionOutcome::ShowResults(vec![yes, no]));
                    }
                    ActionId::OPEN | ActionId::RUN_AS_ADMIN => {}
                    other => return Err(PluginError::UnsupportedAction(other.into())),
                }
                let script = p.catalog.get(&path).ok_or_else(|| {
                    failed(format!(
                        "{}: {}",
                        tr("scripts.script_no_longer_exists"),
                        path.display()
                    ))
                })?;
                let input = str_of("i");
                let parsed = args::parse(&script.meta.args, &input);
                if !parsed.missing.is_empty() {
                    let alias = data["a"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| script.meta.aliases.first().cloned());
                    return Ok(match alias {
                        Some(a) => ActionOutcome::ReplaceQuery(
                            format!("{} {}", a, input.trim()).trim_end().to_owned() + " ",
                        ),
                        None => ActionOutcome::Notify(format!(
                            "{}: {}",
                            tr("scripts.missing_arguments"),
                            parsed.missing.join(", ")
                        )),
                    });
                }
                if let Some(e) = parsed.error {
                    return Ok(ActionOutcome::Notify(e));
                }
                Ok(p.start(
                    script,
                    parsed.values,
                    input,
                    action == ActionId::RUN_AS_ADMIN,
                ))
            }
            "new" => {
                let tpl = templates::by_ext(&str_of("ext"))
                    .ok_or_else(|| failed("plantilla desconocida".into()))?;
                let file = templates::create(p.catalog.dir(), tpl, &str_of("name"))
                    .map_err(|e| failed(e.to_string()))?;
                p.catalog.rescan();
                open_in_editor(&p.settings.editor, &file).map_err(failed)?;
                Ok(ActionOutcome::Hide)
            }
            "delete" => {
                let path = PathBuf::from(str_of("p"));
                rukes_platform::shell::recycle(&path).map_err(|e| failed(e.to_string()))?;
                p.catalog.rescan();
                Ok(ActionOutcome::ReplaceQuery(String::new()))
            }
            "cancel" => {
                let id = data["id"].as_u64().unwrap_or(0);
                Ok(if p.runs.cancel(id) {
                    ActionOutcome::Notify(tr("scripts.cancelling").into())
                } else {
                    ActionOutcome::Notify(tr("scripts.it_had_already_finished").into())
                })
            }
            "log" => {
                let id = data["id"].as_u64().unwrap_or(0);
                let r = p
                    .runs
                    .get(id)
                    .ok_or_else(|| failed("registro no encontrado".into()))?;
                match action {
                    ActionId::COPY_PATH => rukes_plugins::util::copy(&r.output()),
                    ActionId::EDIT => open_in_editor(&p.settings.editor, &r.path)
                        .map(|()| ActionOutcome::Hide)
                        .map_err(failed),
                    _ => Ok(ActionOutcome::ShowResults(ScriptsPlugin::output_items(
                        &r,
                        filter::parse(&r.stdout).as_deref(),
                    ))),
                }
            }
            "open" => rukes_platform::shell::launch(
                &str_of("v"),
                None,
                None,
                rukes_platform::shell::Verb::Open,
            )
            .map(|()| ActionOutcome::Hide)
            .map_err(|e| failed(e.to_string())),
            "copy" => rukes_plugins::util::copy(&str_of("v")),
            "query" => Ok(ActionOutcome::ReplaceQuery(str_of("v"))),
            _ => Ok(ActionOutcome::KeepOpen),
        }
    }

    fn refresh(&self) {
        self.0.catalog.rescan();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    type Delivered = Arc<Mutex<Vec<(String, Vec<ResultItem>)>>>;

    struct Fixture {
        plugin: Scripts,
        dir: PathBuf,
        results: Delivered,
        notes: Arc<Mutex<Vec<(String, String, bool)>>>,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn fixture(name: &str, files: &[(&str, &str)]) -> Fixture {
        let dir = std::env::temp_dir().join(format!("rukes-plug-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (f, text) in files {
            std::fs::write(dir.join(f), text).unwrap();
        }
        let catalog = Catalog::open(dir.clone(), BTreeMap::new(), Arc::new(|| {}));
        let runs = Arc::new(Runs::open(dir.join("runs.json"), 10));
        let results = Arc::new(Mutex::new(Vec::new()));
        let notes = Arc::new(Mutex::new(Vec::new()));
        let (r2, n2) = (Arc::clone(&results), Arc::clone(&notes));
        let hooks = Hooks {
            notify: Arc::new(move |t, b, e| n2.lock().unwrap().push((t.into(), b.into(), e))),
            results: Arc::new(move |k, items| r2.lock().unwrap().push((k, items))),
            context: Arc::new(|| RunContext {
                active_window: "Ventana de prueba".into(),
                ..RunContext::default()
            }),
        };
        let settings = Settings {
            editor: "notepad".into(),
            default_timeout: Duration::from_secs(10),
            interpreters: BTreeMap::new(),
            priority: 0.0,
        };
        Fixture {
            plugin: Scripts(Arc::new(ScriptsPlugin::new(catalog, runs, settings, hooks))),
            dir,
            results,
            notes,
        }
    }

    fn query(p: &Scripts, raw: &str, targeted: bool) -> Vec<ResultItem> {
        let q = Query {
            id: rukes_core::QueryId(1),
            raw: raw.into(),
            text: raw.into(),
            targeted,
        };
        let mut out = Vec::new();
        p.query(&q, &CancelToken::new(), &mut |v| out.extend(v));
        out
    }

    fn wait<T>(m: &Mutex<Vec<T>>, n: usize) {
        let t = Instant::now();
        while m.lock().unwrap().len() < n && t.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    const ECHO: &str = ":: @rukes.name Eco\r\n:: @rukes.alias gh issue\r\n:: @rukes.arg title:text:required\r\n:: @rukes.output launcher\r\n@echo off\r\necho titulo=%~1 ventana=%RUKES_ACTIVE_WINDOW% arg=%RUKES_ARG_TITLE%\r\n";

    #[test]
    fn alias_with_inline_args_runs_and_shows_output() {
        let f = fixture("alias", &[("eco.cmd", ECHO)]);
        assert!(
            f.plugin
                .metadata()
                .keywords
                .contains(&"gh issue".to_owned())
        );
        let items = query(&f.plugin, "gh issue Mi bug", true);
        assert_eq!(items[0].title, "Eco");
        assert!(items[0].alias_match);
        assert_eq!(items[0].subtitle, "title = Mi bug");
        // Sin argumento: Enter completa la búsqueda para escribirlo.
        let empty = query(&f.plugin, "gh issue", false);
        match f.plugin.execute(&empty[0], ActionId::OPEN).unwrap() {
            ActionOutcome::ReplaceQuery(q) => assert_eq!(q, "gh issue "),
            o => panic!("{o:?}"),
        }
        let running = match f.plugin.execute(&items[0], ActionId::OPEN).unwrap() {
            ActionOutcome::ShowResults(v) => v,
            o => panic!("{o:?}"),
        };
        wait(&f.results, 1);
        let (key, out) = f.results.lock().unwrap()[0].clone();
        assert_eq!(key, format!("scripts:{}", running[0].id));
        assert!(out[0].title.starts_with("Eco · ✓"), "{:?}", out[0].title);
        assert_eq!(
            out[1].title,
            "titulo=Mi bug ventana=Ventana de prueba arg=Mi bug"
        );
        wait(&f.notes, 0);
        // Queda en el historial.
        let logs = query(&f.plugin, "logs", false);
        assert_eq!(logs[0].title, "Eco");
        assert!(logs[0].subtitle.starts_with('✓'));
    }

    #[test]
    fn hung_script_is_killed_by_its_timeout() {
        let f = fixture(
            "hang",
            &[(
                "cuelga.cmd",
                ":: @rukes.name Cuelga\r\n:: @rukes.alias cuelga\r\n:: @rukes.timeout 500ms\r\n:: @rukes.output silent\r\nping -n 60 127.0.0.1 >nul\r\n",
            )],
        );
        let items = query(&f.plugin, "cuelga", false);
        let t = Instant::now();
        assert!(matches!(
            f.plugin.execute(&items[0], ActionId::OPEN).unwrap(),
            ActionOutcome::Hide
        ));
        wait(&f.notes, 1);
        assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
        let (title, body, error) = f.notes.lock().unwrap()[0].clone();
        assert_eq!(title, "Cuelga");
        assert!(error);
        assert!(body.contains('⏱'), "{body}");
        let t = Instant::now();
        while f.history_len() == 0 && t.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(f.plugin.0.runs.history()[0].status, Status::Timeout);
    }

    impl Fixture {
        fn history_len(&self) -> usize {
            self.plugin.0.runs.history().len()
        }
    }

    #[test]
    fn script_filter_returns_json_items() {
        let f = fixture(
            "filter",
            &[(
                "f.cmd",
                ":: @rukes.name Filtro\r\n:: @rukes.alias ff\r\n:: @rukes.filter true\r\n@echo off\r\necho [{\"title\":\"uno %RUKES_QUERY%\",\"open\":\"https://example.com\"},{\"title\":\"dos\"}]\r\n",
            )],
        );
        let items = query(&f.plugin, "ff hola", true);
        assert_eq!(items.len(), 2, "{items:?}");
        assert_eq!(items[0].title, "uno hola");
        match f.plugin.execute(&items[1], ActionId::OPEN) {
            Ok(ActionOutcome::Notify(_)) | Err(_) => {}
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn fuzzy_search_templates_and_choices() {
        let f = fixture(
            "misc",
            &[(
                "d.cmd",
                ":: @rukes.name Desplegar app\r\n:: @rukes.alias deploy\r\n:: @rukes.arg env:option(dev|prod):required\r\n",
            )],
        );
        assert_eq!(query(&f.plugin, "despl", false)[0].title, "Desplegar app");
        let choices = query(&f.plugin, "deploy ", true);
        let titles: Vec<&str> = choices.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Desplegar app", "Desplegar app: dev", "Desplegar app: prod"]
        );
        let new = query(&f.plugin, "new script Backup fotos", true);
        assert_eq!(new.len(), TEMPLATES.len());
        assert!(new[0].subtitle.starts_with("backup-fotos.ps1"));
        // Borrar pide confirmación.
        assert!(matches!(
            f.plugin.execute(&choices[0], ActionId::DELETE).unwrap(),
            ActionOutcome::ShowResults(v) if v.len() == 2
        ));
    }
}
