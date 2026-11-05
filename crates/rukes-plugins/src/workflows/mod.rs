//! Workflows y triggers (secciones 5.14 y 5.15, RF-150 a RF-155, RF-160, RF-162, RF-163).
//!
//! Se leen de los `.toml` de la carpeta `workflows` de la configuración:
//!
//! ```toml
//! [workflow.trabajo]
//! name = "Modo trabajo"
//! alias = ["work", "trabajo"]
//! hotkey = "Ctrl+Alt+1"
//! steps = [
//!   { app = "code", args = "C:/proyectos/actual", window = { monitor = 1, snap = "left" } },
//!   { url = "https://docs.rs", window = { monitor = 2, snap = "right" } },
//!   { script = "start-server" },
//!   { power_plan = "high_performance" },
//! ]
//!
//! [[trigger]]
//! at = "09:00"
//! days = ["laborables"]
//! run = "workflow:trabajo"
//! ```

mod plugin;
mod run;
mod triggers;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use rukes_core::i18n::{tr, trf};
use rukes_platform::monitor::Rect;
use serde::Deserialize;

pub use plugin::WorkflowsPlugin;
pub use run::{Hooks, Runner, StepReport};
pub use triggers::{Scheduler, next_due, weekday};

use crate::util::ellipsis;

// ---------------------------------------------------------------- formato

/// Número o texto (`800` o `"50%"`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Dim {
    /// Píxeles.
    Px(i64),
    /// `"50%"` o `"800"`.
    Text(String),
}

/// Texto o lista de textos.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    /// Uno.
    One(String),
    /// Varios.
    Many(Vec<String>),
}

impl OneOrMany {
    fn list(&self) -> Vec<String> {
        match self {
            Self::One(s) => vec![s.clone()],
            Self::Many(v) => v.clone(),
        }
    }
}

/// Posición de la ventana de un paso (RF-151).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowDef {
    /// Monitor (1 = principal, luego de izquierda a derecha).
    pub monitor: Option<usize>,
    /// `left`, `right`, `top`, `bottom`, `top-left`… `left-third`, `center`, `full`.
    pub snap: Option<String>,
    /// Posición y tamaño dentro del área de trabajo: píxeles o porcentaje.
    pub x: Option<Dim>,
    /// Ver `x`.
    pub y: Option<Dim>,
    /// Ver `x`.
    pub width: Option<Dim>,
    /// Ver `x`.
    pub height: Option<Dim>,
    /// Maximizar en el monitor.
    #[serde(default)]
    pub maximized: bool,
    /// Texto del título (o del ejecutable) para reconocer la ventana.
    pub title: Option<String>,
    /// Cuánto esperar a que aparezca la ventana (`10s`).
    pub wait: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct StepDef {
    #[serde(alias = "open")]
    app: Option<String>,
    url: Option<String>,
    script: Option<String>,
    wait: Option<String>,
    theme: Option<String>,
    power_plan: Option<String>,
    kill: Option<OneOrMany>,
    notify: Option<String>,
    args: Option<OneOrMany>,
    cwd: Option<String>,
    #[serde(default)]
    admin: bool,
    window: Option<WindowDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowDef {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    alias: Vec<String>,
    hotkey: Option<String>,
    #[serde(default)]
    steps: Vec<StepDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct TriggerDef {
    name: Option<String>,
    at: Option<String>,
    every: Option<String>,
    on: Option<String>,
    days: Option<Vec<String>>,
    run: String,
    #[serde(default = "yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDef {
    #[serde(default)]
    workflow: BTreeMap<String, WorkflowDef>,
    #[serde(default)]
    trigger: Vec<TriggerDef>,
}

// ---------------------------------------------------------------- modelo

/// Qué hace un paso.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Abrir una app, archivo, carpeta o acceso directo.
    App {
        /// Destino (`code`, ruta, `shell:AppsFolder\…`).
        target: String,
        /// Argumentos.
        args: Option<String>,
        /// Carpeta de trabajo.
        cwd: Option<String>,
        /// Como administrador.
        admin: bool,
    },
    /// Abrir una URL.
    Url(String),
    /// Ejecutar un script del usuario.
    Script {
        /// Nombre, alias o archivo.
        name: String,
        /// Argumentos.
        args: Vec<String>,
    },
    /// Esperar.
    Wait(Duration),
    /// Cambiar el tema de Rukes.
    Theme(String),
    /// Cambiar el plan de energía.
    PowerPlan(String),
    /// Cerrar procesos por nombre de ejecutable.
    Kill(Vec<String>),
    /// Mostrar una notificación.
    Notify(String),
}

/// Un paso.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// Acción.
    pub action: Action,
    /// Dónde colocar la ventana que abre.
    pub window: Option<WindowDef>,
}

/// Un workflow.
#[derive(Debug, Clone, PartialEq)]
pub struct Workflow {
    /// Clave en el archivo (`trabajo`).
    pub id: String,
    /// Nombre visible.
    pub name: String,
    /// Descripción.
    pub description: Option<String>,
    /// Alias (minúsculas).
    pub alias: Vec<String>,
    /// Atajo global propio.
    pub hotkey: Option<String>,
    /// Pasos.
    pub steps: Vec<Step>,
    /// Archivo de origen.
    pub path: PathBuf,
}

/// Qué ejecuta un trigger o un atajo.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// Un workflow (por id, nombre o alias).
    Workflow(String),
    /// Un script (por nombre, alias o archivo).
    Script(String),
}

impl Target {
    /// `workflow:trabajo`, `script:backup` o un nombre suelto (workflow si existe, si no script).
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if let Some(w) = s.strip_prefix("workflow:") {
            Self::Workflow(w.trim().to_owned())
        } else if let Some(sc) = s.strip_prefix("script:") {
            Self::Script(sc.trim().to_owned())
        } else {
            Self::Workflow(s.to_owned())
        }
    }
}

/// Cuándo salta un trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum When {
    /// Cada día a esta hora (`(hora, minuto)`).
    At(u32, u32),
    /// Cada intervalo.
    Every(Duration),
    /// Al arrancar Rukes.
    Startup,
    /// Al iniciar sesión (Rukes arrancado en los primeros minutos tras encender).
    Login,
}

/// Un trigger programado (RF-160).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    /// Nombre visible.
    pub name: String,
    /// Cuándo.
    pub when: When,
    /// Días de la semana permitidos (0 = domingo). Vacío = todos.
    pub days: Vec<u32>,
    /// Qué ejecuta.
    pub target: Target,
    /// Activo.
    pub enabled: bool,
}

impl Trigger {
    /// Clave estable para recordar su próxima ejecución entre recargas.
    pub fn key(&self) -> String {
        format!("{:?}|{:?}|{:?}", self.when, self.days, self.target)
    }

    /// «cada día a las 09:00», «cada 30 min», «al iniciar sesión».
    pub fn when_label(&self) -> String {
        match &self.when {
            When::At(h, m) => trf("workflows.at", &[("time", &format!("{h:02}:{m:02}"))]),
            When::Every(d) => trf("workflows.every", &[("every", &human_duration(*d))]),
            When::Startup => tr("workflows.on_startup").into(),
            When::Login => tr("workflows.on_login").into(),
        }
    }
}

/// `90s` → «1 min 30 s».
pub fn human_duration(d: Duration) -> String {
    let s = d.as_secs();
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(format!("{h} h"));
    }
    if m > 0 {
        parts.push(format!("{m} min"));
    }
    if sec > 0 || parts.is_empty() {
        if parts.is_empty() && d.subsec_millis() > 0 {
            parts.push(format!("{} ms", d.as_millis()));
        } else {
            parts.push(format!("{sec} s"));
        }
    }
    parts.join(" ")
}

/// Duración `500ms`, `2s`, `1m30s`, `1h`, o un número de segundos.
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    if let Ok(n) = s.parse::<f64>() {
        return (n >= 0.0).then(|| Duration::from_secs_f64(n));
    }
    if let Some(ms) = s.strip_suffix("ms") {
        return ms.trim().parse::<u64>().ok().map(Duration::from_millis);
    }
    let mut total = 0f64;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' || c == ',' {
            num.push(if c == ',' { '.' } else { c });
            continue;
        }
        if c.is_whitespace() {
            continue;
        }
        let n: f64 = num.parse().ok()?;
        num.clear();
        total += n * match c {
            's' => 1.0,
            'm' => 60.0,
            'h' => 3600.0,
            'd' => 86_400.0,
            _ => return None,
        };
    }
    if !num.is_empty() {
        return None;
    }
    Some(Duration::from_secs_f64(total))
}

/// `"09:00"` → `(9, 0)`.
fn parse_clock(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// Días: `lun`, `monday`, `laborables`, `finde`… (0 = domingo).
fn parse_days(list: &[String]) -> Result<Vec<u32>, String> {
    let mut out = Vec::new();
    for d in list {
        let d = crate::util::fold(d.trim());
        let add: &[u32] = match d.as_str() {
            "dom" | "domingo" | "sun" | "sunday" => &[0],
            "lun" | "lunes" | "mon" | "monday" => &[1],
            "mar" | "martes" | "tue" | "tuesday" => &[2],
            "mie" | "miercoles" | "wed" | "wednesday" => &[3],
            "jue" | "jueves" | "thu" | "thursday" => &[4],
            "vie" | "viernes" | "fri" | "friday" => &[5],
            "sab" | "sabado" | "sat" | "saturday" => &[6],
            "laborables" | "weekdays" => &[1, 2, 3, 4, 5],
            "finde" | "fin de semana" | "weekend" => &[0, 6],
            _ => return Err(trf("workflows.bad_day", &[("day", &d)])),
        };
        for x in add {
            if !out.contains(x) {
                out.push(*x);
            }
        }
    }
    out.sort_unstable();
    Ok(out)
}

/// Separa argumentos por espacios respetando comillas dobles.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

fn step_from(def: StepDef) -> Result<Step, String> {
    let kinds = [
        def.app.is_some(),
        def.url.is_some(),
        def.script.is_some(),
        def.wait.is_some(),
        def.theme.is_some(),
        def.power_plan.is_some(),
        def.kill.is_some(),
        def.notify.is_some(),
    ];
    match kinds.iter().filter(|k| **k).count() {
        0 => return Err(tr("workflows.step_empty").into()),
        1 => {}
        _ => return Err(tr("workflows.step_many").into()),
    }
    let args_text = def.args.as_ref().map(|a| match a {
        OneOrMany::One(s) => s.clone(),
        OneOrMany::Many(v) => v
            .iter()
            .map(|x| {
                if x.contains(' ') {
                    format!("\"{x}\"")
                } else {
                    x.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    });
    let action = if let Some(target) = def.app {
        Action::App {
            target,
            args: args_text,
            cwd: def.cwd,
            admin: def.admin,
        }
    } else if let Some(u) = def.url {
        Action::Url(u)
    } else if let Some(name) = def.script {
        Action::Script {
            name,
            args: match def.args {
                Some(OneOrMany::One(s)) => split_args(&s),
                Some(OneOrMany::Many(v)) => v,
                None => Vec::new(),
            },
        }
    } else if let Some(w) = def.wait {
        Action::Wait(
            parse_duration(&w).ok_or_else(|| trf("workflows.bad_duration", &[("value", &w)]))?,
        )
    } else if let Some(t) = def.theme {
        Action::Theme(t)
    } else if let Some(p) = def.power_plan {
        Action::PowerPlan(p)
    } else if let Some(k) = def.kill {
        Action::Kill(k.list())
    } else if let Some(n) = def.notify {
        Action::Notify(n)
    } else {
        return Err(tr("workflows.step_empty").into());
    };
    if def.window.is_some() && !matches!(action, Action::App { .. } | Action::Url(_)) {
        return Err(tr("workflows.window_only_open").into());
    }
    if let Some(w) = &def.window {
        // Valida la posición con un área cualquiera.
        layout(
            Rect {
                left: 0,
                top: 0,
                right: 1000,
                bottom: 1000,
            },
            w,
        )?;
    }
    Ok(Step {
        action,
        window: def.window,
    })
}

fn trigger_from(def: TriggerDef) -> Result<Trigger, String> {
    let set = [def.at.is_some(), def.every.is_some(), def.on.is_some()];
    if set.iter().filter(|x| **x).count() != 1 {
        return Err(tr("workflows.trigger_when").into());
    }
    let when = if let Some(a) = &def.at {
        let (h, m) = parse_clock(a).ok_or_else(|| trf("workflows.bad_time", &[("value", a)]))?;
        When::At(h, m)
    } else if let Some(e) = &def.every {
        let d = parse_duration(e)
            .filter(|d| d.as_secs() >= 60)
            .ok_or_else(|| trf("workflows.bad_every", &[("value", e)]))?;
        When::Every(d)
    } else {
        match def.on.as_deref().map(str::trim) {
            Some("startup" | "inicio" | "arranque") => When::Startup,
            Some("login" | "sesion" | "sesión") => When::Login,
            other => {
                return Err(trf(
                    "workflows.bad_on",
                    &[("value", &other.unwrap_or_default())],
                ));
            }
        }
    };
    let days = parse_days(def.days.as_deref().unwrap_or_default())?;
    let target = Target::parse(&def.run);
    Ok(Trigger {
        name: def.name.unwrap_or_else(|| def.run.trim().to_owned()),
        when,
        days,
        target,
        enabled: def.enabled,
    })
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

/// Lee un archivo: workflows, triggers y errores (`archivo:línea: mensaje`).
pub fn parse_file(text: &str, path: &Path) -> (Vec<Workflow>, Vec<Trigger>, Vec<String>) {
    let mut errors = Vec::new();
    let file: FileDef = match toml::from_str(text) {
        Ok(f) => f,
        Err(e) => {
            let line = e.span().map_or(0, |s| line_of(text, s.start));
            errors.push(format!("{}:{line}: {}", path.display(), e.message().trim()));
            return (Vec::new(), Vec::new(), errors);
        }
    };
    let where_is = |needle: &str| text.find(needle).map_or(0, |o| line_of(text, o));
    let mut workflows = Vec::new();
    for (id, def) in file.workflow {
        let line = where_is(&format!("workflow.{id}"));
        let mut steps = Vec::new();
        let mut bad = false;
        for (i, s) in def.steps.into_iter().enumerate() {
            match step_from(s) {
                Ok(s) => steps.push(s),
                Err(e) => {
                    errors.push(format!(
                        "{}:{line}: [workflow.{id}] {} {}: {e}",
                        path.display(),
                        tr("workflows.step"),
                        i + 1
                    ));
                    bad = true;
                }
            }
        }
        if bad {
            continue;
        }
        workflows.push(Workflow {
            name: def.name.unwrap_or_else(|| id.clone()),
            description: def.description,
            alias: def
                .alias
                .iter()
                .map(|a| a.trim().to_lowercase())
                .filter(|a| !a.is_empty())
                .collect(),
            hotkey: def.hotkey.filter(|h| !h.trim().is_empty()),
            steps,
            path: path.to_owned(),
            id,
        });
    }
    let mut triggers = Vec::new();
    for (i, def) in file.trigger.into_iter().enumerate() {
        match trigger_from(def) {
            Ok(t) => triggers.push(t),
            Err(e) => errors.push(format!("{}: [[trigger]] {}: {e}", path.display(), i + 1)),
        }
    }
    (workflows, triggers, errors)
}

// ---------------------------------------------------------------- posición

fn dim(d: &Dim, total: i32) -> Result<i32, String> {
    let v = match d {
        Dim::Px(n) => i32::try_from(*n).map_err(|_| trf("workflows.bad_dim", &[("value", n)]))?,
        Dim::Text(s) => {
            let t = s.trim();
            if let Some(p) = t.strip_suffix('%') {
                let p: f32 = p
                    .trim()
                    .replace(',', ".")
                    .parse()
                    .map_err(|_| trf("workflows.bad_dim", &[("value", s)]))?;
                (total as f32 * p / 100.0).round() as i32
            } else {
                t.trim_end_matches("px")
                    .trim()
                    .parse()
                    .map_err(|_| trf("workflows.bad_dim", &[("value", s)]))?
            }
        }
    };
    Ok(v)
}

/// Rectángulo final dentro del área de trabajo `work` y si hay que maximizar.
pub fn layout(work: Rect, w: &WindowDef) -> Result<(Rect, bool), String> {
    let (x0, y0, ww, wh) = (work.left, work.top, work.width(), work.height());
    let part = |fx: f32, fy: f32, fw: f32, fh: f32| Rect {
        left: x0 + (ww as f32 * fx).round() as i32,
        top: y0 + (wh as f32 * fy).round() as i32,
        right: x0 + (ww as f32 * (fx + fw)).round() as i32,
        bottom: y0 + (wh as f32 * (fy + fh)).round() as i32,
    };
    if w.maximized {
        return Ok((work, true));
    }
    if let Some(snap) = &w.snap {
        let third = 1.0 / 3.0;
        let r = match snap.trim().to_ascii_lowercase().as_str() {
            "left" | "izquierda" => part(0.0, 0.0, 0.5, 1.0),
            "right" | "derecha" => part(0.5, 0.0, 0.5, 1.0),
            "top" | "arriba" => part(0.0, 0.0, 1.0, 0.5),
            "bottom" | "abajo" => part(0.0, 0.5, 1.0, 0.5),
            "top-left" => part(0.0, 0.0, 0.5, 0.5),
            "top-right" => part(0.5, 0.0, 0.5, 0.5),
            "bottom-left" => part(0.0, 0.5, 0.5, 0.5),
            "bottom-right" => part(0.5, 0.5, 0.5, 0.5),
            "left-third" => part(0.0, 0.0, third, 1.0),
            "center-third" => part(third, 0.0, third, 1.0),
            "right-third" => part(2.0 * third, 0.0, third, 1.0),
            "left-two-thirds" => part(0.0, 0.0, 2.0 * third, 1.0),
            "right-two-thirds" => part(third, 0.0, 2.0 * third, 1.0),
            "center" | "centro" => part(0.15, 0.1, 0.7, 0.8),
            "full" | "completa" => work,
            "maximize" | "maximized" | "maximizar" => return Ok((work, true)),
            other => return Err(trf("workflows.bad_snap", &[("value", &other)])),
        };
        return Ok((r, false));
    }
    if w.x.is_none() && w.y.is_none() && w.width.is_none() && w.height.is_none() {
        // Solo `monitor`: centrada en ese monitor.
        return Ok((part(0.15, 0.1, 0.7, 0.8), false));
    }
    let width = w.width.as_ref().map_or(Ok(ww * 7 / 10), |d| dim(d, ww))?;
    let height = w.height.as_ref().map_or(Ok(wh * 8 / 10), |d| dim(d, wh))?;
    if width <= 0 || height <= 0 {
        return Err(tr("workflows.bad_size").into());
    }
    let x = w.x.as_ref().map_or(Ok((ww - width) / 2), |d| dim(d, ww))?;
    let y = w.y.as_ref().map_or(Ok((wh - height) / 2), |d| dim(d, wh))?;
    Ok((
        Rect {
            left: x0 + x,
            top: y0 + y,
            right: x0 + x + width,
            bottom: y0 + y + height,
        },
        false,
    ))
}

// ---------------------------------------------------------------- textos

impl Step {
    /// «Abrir code → monitor 1, left».
    pub fn label(&self) -> String {
        let base = match &self.action {
            Action::App { target, args, .. } => {
                let t = match args {
                    Some(a) => format!("{target} {a}"),
                    None => target.clone(),
                };
                trf("workflows.step_app", &[("target", &ellipsis(&t, 60))])
            }
            Action::Url(u) => trf("workflows.step_url", &[("url", &ellipsis(u, 60))]),
            Action::Script { name, args } => {
                let t = if args.is_empty() {
                    name.clone()
                } else {
                    format!("{name} {}", args.join(" "))
                };
                trf("workflows.step_script", &[("name", &ellipsis(&t, 60))])
            }
            Action::Wait(d) => trf("workflows.step_wait", &[("time", &human_duration(*d))]),
            Action::Theme(t) => trf("workflows.step_theme", &[("theme", t)]),
            Action::PowerPlan(p) => trf("workflows.step_power", &[("plan", p)]),
            Action::Kill(list) => trf("workflows.step_kill", &[("names", &list.join(", "))]),
            Action::Notify(t) => trf("workflows.step_notify", &[("text", &ellipsis(t, 60))]),
        };
        match &self.window {
            Some(w) => {
                let mut where_ = Vec::new();
                if let Some(m) = w.monitor {
                    where_.push(trf("workflows.monitor", &[("n", &m)]));
                }
                if w.maximized {
                    where_.push(tr("workflows.maximized").into());
                } else if let Some(s) = &w.snap {
                    where_.push(s.clone());
                } else if w.x.is_some() || w.width.is_some() {
                    where_.push(tr("workflows.custom_rect").into());
                }
                if where_.is_empty() {
                    base
                } else {
                    format!("{base} → {}", where_.join(", "))
                }
            }
            None => base,
        }
    }
}

// ---------------------------------------------------------------- almacén

/// Workflows y triggers cargados de la carpeta, con el estado de ejecución.
pub struct WorkflowStore {
    dir: PathBuf,
    workflows: RwLock<Vec<Workflow>>,
    triggers: RwLock<Vec<Trigger>>,
    /// Último informe por workflow: `(unix, pasos)`.
    last: Mutex<BTreeMap<String, (u64, Vec<StepReport>)>>,
    /// Próxima ejecución de cada trigger (unix), por [`Trigger::key`].
    pub(crate) next: Mutex<BTreeMap<String, u64>>,
}

/// Ejemplos que se copian al crear la carpeta.
const EXAMPLE: &str = include_str!("../../workflows/ejemplos.toml");

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl WorkflowStore {
    /// Abre la carpeta (la crea con ejemplos si no existe) y la lee. Devuelve también los errores.
    pub fn open(dir: PathBuf) -> (Arc<Self>, Vec<String>) {
        if !dir.exists() && std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(dir.join("ejemplos.toml"), EXAMPLE);
        }
        let s = Arc::new(Self {
            dir,
            workflows: RwLock::new(Vec::new()),
            triggers: RwLock::new(Vec::new()),
            last: Mutex::new(BTreeMap::new()),
            next: Mutex::new(BTreeMap::new()),
        });
        let errors = s.reload();
        (s, errors)
    }

    /// Carpeta.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Vuelve a leer la carpeta. Devuelve los errores.
    pub fn reload(&self) -> Vec<String> {
        let mut wf = Vec::new();
        let mut tg = Vec::new();
        let mut errors = Vec::new();
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        for p in files {
            if !p
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let (w, t, e) = parse_file(text.trim_start_matches('\u{feff}'), &p);
            for x in w {
                if wf.iter().any(|y: &Workflow| y.id == x.id) {
                    errors.push(trf("workflows.duplicate", &[("id", &x.id)]));
                } else {
                    wf.push(x);
                }
            }
            tg.extend(t);
            errors.extend(e);
        }
        tracing::info!(
            workflows = wf.len(),
            triggers = tg.len(),
            errors = errors.len(),
            "workflows cargados"
        );
        *self
            .workflows
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = wf;
        *self
            .triggers
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = tg;
        errors
    }

    /// Workflows.
    pub fn workflows(&self) -> Vec<Workflow> {
        self.workflows
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Triggers.
    pub fn triggers(&self) -> Vec<Trigger> {
        self.triggers
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Busca por id, nombre o alias (sin distinguir mayúsculas).
    pub fn find(&self, name: &str) -> Option<Workflow> {
        let n = name.trim().to_lowercase();
        self.workflows().into_iter().find(|w| {
            w.id.to_lowercase() == n || w.name.to_lowercase() == n || w.alias.contains(&n)
        })
    }

    /// Guarda el informe de una ejecución.
    pub fn set_report(&self, id: &str, report: Vec<StepReport>) {
        lock(&self.last).insert(id.to_owned(), (crate::util::now_unix(), report));
    }

    /// Último informe.
    pub fn report(&self, id: &str) -> Option<(u64, Vec<StepReport>)> {
        lock(&self.last).get(id).cloned()
    }

    /// Próxima ejecución (unix) de un trigger, si el planificador la calculó.
    pub fn next_run(&self, t: &Trigger) -> Option<u64> {
        lock(&self.next).get(&t.key()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> Rect {
        Rect {
            left: 1920,
            top: 0,
            right: 3840,
            bottom: 1040,
        }
    }

    fn win(snap: &str) -> WindowDef {
        WindowDef {
            snap: Some(snap.into()),
            ..WindowDef::default()
        }
    }

    #[test]
    fn layouts() {
        let (r, max) = layout(work(), &win("left")).unwrap();
        assert!(!max);
        assert_eq!((r.left, r.top, r.width(), r.height()), (1920, 0, 960, 1040));
        let (r, _) = layout(work(), &win("right")).unwrap();
        assert_eq!((r.left, r.right), (2880, 3840));
        let (r, _) = layout(work(), &win("bottom-right")).unwrap();
        assert_eq!((r.left, r.top), (2880, 520));
        let (r, _) = layout(work(), &win("right-third")).unwrap();
        assert_eq!(r.right, 3840);
        assert!(layout(work(), &win("maximize")).unwrap().1);
        assert!(layout(work(), &win("diagonal")).is_err());
        let w = WindowDef {
            x: Some(Dim::Text("10%".into())),
            y: Some(Dim::Px(20)),
            width: Some(Dim::Text("50%".into())),
            height: Some(Dim::Px(600)),
            ..WindowDef::default()
        };
        let (r, _) = layout(work(), &w).unwrap();
        assert_eq!((r.left, r.top, r.width(), r.height()), (2112, 20, 960, 600));
        let w = WindowDef {
            width: Some(Dim::Px(800)),
            height: Some(Dim::Px(600)),
            ..WindowDef::default()
        };
        let (r, _) = layout(work(), &w).unwrap();
        assert_eq!((r.left, r.top), (1920 + 560, 220));
        assert!(
            layout(
                work(),
                &WindowDef {
                    width: Some(Dim::Text("ancho".into())),
                    ..WindowDef::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn durations_and_args() {
        assert_eq!(parse_duration("2s"), Some(Duration::from_secs(2)));
        assert_eq!(parse_duration("1m30s"), Some(Duration::from_secs(90)));
        assert_eq!(parse_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("1.5"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_duration("5x"), None);
        assert_eq!(human_duration(Duration::from_secs(90)), "1 min 30 s");
        assert_eq!(
            split_args(r#"uno "dos tres" cuatro"#),
            ["uno", "dos tres", "cuatro"]
        );
        assert_eq!(split_args(r#""""#), [""]);
    }

    #[test]
    fn parses_example_file() {
        let _ = rukes_core::i18n::set_language("es", "");
        let (w, t, e) = parse_file(EXAMPLE, Path::new("ejemplos.toml"));
        assert!(e.is_empty(), "{e:?}");
        let names: Vec<&str> = w.iter().map(|w| w.id.as_str()).collect();
        for id in ["trabajo", "gaming", "presentacion", "proyecto_x"] {
            assert!(names.contains(&id), "{id}");
        }
        assert!(t.is_empty() || t.iter().all(|t| !t.enabled));
        let trabajo = w.iter().find(|w| w.id == "trabajo").unwrap();
        assert!(trabajo.alias.contains(&"work".to_owned()));
        assert!(trabajo.steps.iter().any(|s| s.window.is_some()));
    }

    #[test]
    fn parses_steps_triggers_and_reports_errors() {
        let _ = rukes_core::i18n::set_language("es", "");
        let text = r#"
[workflow.a]
name = "A"
alias = ["Uno"]
hotkey = "Ctrl+Alt+1"
steps = [
  { app = "code", args = ["--new-window", "C:/mi carpeta"], window = { monitor = 2, snap = "left" } },
  { url = "https://docs.rs" },
  { script = "backup", args = "uno \"dos tres\"" },
  { wait = "1s" },
  { theme = "nord" },
  { power_plan = "balanced" },
  { kill = "notepad.exe" },
  { notify = "listo" },
]

[workflow.b]
steps = [ { app = "x", url = "y" } ]

[workflow.c]
steps = [ { wait = "1s", window = { snap = "left" } } ]

[[trigger]]
at = "09:30"
days = ["laborables"]
run = "workflow:a"

[[trigger]]
every = "30m"
run = "script:backup"

[[trigger]]
on = "login"
run = "a"

[[trigger]]
every = "10s"
run = "a"

[[trigger]]
at = "25:00"
run = "a"
"#;
        let (w, t, e) = parse_file(text, Path::new("w.toml"));
        assert_eq!(w.len(), 1);
        let a = &w[0];
        assert_eq!(a.alias, ["uno"]);
        assert_eq!(a.steps.len(), 8);
        assert_eq!(
            a.steps[0].action,
            Action::App {
                target: "code".into(),
                args: Some("--new-window \"C:/mi carpeta\"".into()),
                cwd: None,
                admin: false,
            }
        );
        assert_eq!(
            a.steps[2].action,
            Action::Script {
                name: "backup".into(),
                args: vec!["uno".into(), "dos tres".into()],
            }
        );
        assert!(a.steps[0].label().contains("monitor 2, left"));
        assert_eq!(e.len(), 4, "{e:?}");
        assert!(
            e[0].contains("workflow.b") && e[0].contains(":17:"),
            "{e:?}"
        );
        assert!(e[1].contains("workflow.c"));
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].when, When::At(9, 30));
        assert_eq!(t[0].days, [1, 2, 3, 4, 5]);
        assert_eq!(t[1].target, Target::Script("backup".into()));
        assert_eq!(t[2].when, When::Login);
        assert_eq!(t[2].target, Target::Workflow("a".into()));
        let (_, _, e) = parse_file("[workflow.x\n", Path::new("roto.toml"));
        assert!(e[0].starts_with("roto.toml:1:"), "{e:?}");
    }

    #[test]
    fn store_finds_by_alias_and_reloads() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = std::env::temp_dir().join(format!("rukes-wf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (store, errors) = WorkflowStore::open(dir.clone());
        assert!(errors.is_empty(), "{errors:?}");
        assert!(dir.join("ejemplos.toml").exists());
        assert!(store.find("WORK").is_some());
        assert!(store.find("Modo trabajo").is_some());
        std::fs::write(
            dir.join("mio.toml"),
            "[workflow.mio]\nsteps = [{ notify = \"hola\" }]\n",
        )
        .unwrap();
        assert!(store.reload().is_empty());
        assert!(store.find("mio").is_some());
        std::fs::write(
            dir.join("dup.toml"),
            "[workflow.mio]\nsteps = [{ notify = \"otra\" }]\n",
        )
        .unwrap();
        assert_eq!(store.reload().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
