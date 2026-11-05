//! Ejecución de workflows paso a paso con informe (RF-150, RF-151, RF-155).

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rukes_core::i18n::{tr, trf};
use rukes_platform::Hwnd;
use rukes_platform::monitor::{self, Monitor};
use rukes_platform::winlist::{self, WindowInfo};

use super::{Action, Step, Workflow, WorkflowStore, layout};

/// Resultado de un paso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepReport {
    /// Texto del paso.
    pub label: String,
    /// Terminó bien.
    pub ok: bool,
    /// Detalle (error, ventana usada, avisos).
    pub detail: String,
}

/// Ejecuta un script: `(nombre, argumentos)`.
pub type RunScript = Arc<dyn Fn(&str, &[String]) -> Result<(), String> + Send + Sync>;
/// Cambia el tema de Rukes.
pub type SetTheme = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
/// Notificación: `(título, texto, error)`.
pub type Notify = Arc<dyn Fn(&str, &str, bool) + Send + Sync>;

/// Lo que el ejecutor pide al host.
#[derive(Clone)]
pub struct Hooks {
    /// Ejecuta un script del usuario por nombre, alias o archivo.
    pub run_script: RunScript,
    /// Cambia el tema de Rukes.
    pub set_theme: SetTheme,
    /// Notificación: `(título, texto, error)`.
    pub notify: Notify,
}

/// Ejecutor. Un workflow no se ejecuta dos veces a la vez.
pub struct Runner {
    store: Arc<WorkflowStore>,
    hooks: Hooks,
    running: Mutex<BTreeSet<String>>,
}

/// Espera por defecto a que aparezca la ventana de una app o de una URL.
const APP_WAIT: Duration = Duration::from_secs(10);
const URL_WAIT: Duration = Duration::from_secs(4);

/// Sustituye `%VAR%` y un `~` inicial.
pub fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    if let Some(r) = rest.strip_prefix('~')
        && (r.is_empty() || r.starts_with(['/', '\\']))
        && let Some(home) = std::env::var_os("USERPROFILE")
    {
        out.push_str(&home.to_string_lossy());
        rest = r;
    }
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 && !after[..j].contains(char::is_whitespace) => {
                match std::env::var(&after[..j]) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => out.push_str(&rest[i..i + j + 2]),
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn stem(s: &str) -> String {
    let file = s.rsplit(['/', '\\']).next().unwrap_or(s);
    file.trim_end_matches(".exe")
        .trim_end_matches(".EXE")
        .to_lowercase()
}

fn exe_stem(w: &WindowInfo) -> String {
    w.exe
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Elige la ventana del paso entre las que aparecieron (`fresh`) o, si no hubo, entre todas.
fn pick(
    fresh: &[WindowInfo],
    all: &[WindowInfo],
    pid: Option<u32>,
    target_stem: &str,
    title: Option<&str>,
    is_url: bool,
) -> Option<Hwnd> {
    let title = title.map(str::to_lowercase);
    let by_title = |w: &&WindowInfo| {
        title
            .as_deref()
            .is_some_and(|t| w.title.to_lowercase().contains(t) || exe_stem(w).contains(t))
    };
    let by_app = |w: &&WindowInfo| {
        pid.is_some_and(|p| w.pid == p) || (!target_stem.is_empty() && exe_stem(w) == target_stem)
    };
    fresh
        .iter()
        .find(by_title)
        .or_else(|| fresh.iter().find(by_app))
        .or_else(|| if title.is_none() { fresh.first() } else { None })
        .or_else(|| all.iter().find(by_title))
        .or_else(|| all.iter().find(by_app))
        // Una URL en un navegador ya abierto abre una pestaña: la ventana del navegador queda
        // arriba del todo.
        .or_else(|| if is_url { all.first() } else { None })
        .map(|w| w.hwnd)
}

/// Monitor `n` (desde 1); si no existe, el último. Devuelve también el aviso.
fn monitor_for(n: Option<usize>) -> Option<(Monitor, Option<String>)> {
    let list = monitor::monitors();
    let last = *list.last()?;
    match n {
        None => Some((list[0], None)),
        Some(n) => match list.get(n.saturating_sub(1)) {
            Some(m) if n >= 1 => Some((*m, None)),
            _ => Some((
                last,
                Some(trf(
                    "workflows.no_monitor",
                    &[("n", &n), ("used", &list.len())],
                )),
            )),
        },
    }
}

fn power_plan_arg(p: &str) -> String {
    match crate::util::fold(p.trim())
        .replace([' ', '-'], "_")
        .as_str()
    {
        "high_performance" | "alto_rendimiento" | "performance" => "SCHEME_MIN".into(),
        "balanced" | "equilibrado" => "SCHEME_BALANCED".into(),
        "power_saver" | "ahorro" | "economizador" | "ahorro_de_energia" => "SCHEME_MAX".into(),
        _ => p.trim().to_owned(),
    }
}

fn run_hidden(program: &str, args: &[&str]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        Err(if msg.is_empty() {
            format!("{program}: {}", out.status)
        } else {
            msg
        })
    }
}

impl Runner {
    /// Crea el ejecutor.
    pub fn new(store: Arc<WorkflowStore>, hooks: Hooks) -> Arc<Self> {
        Arc::new(Self {
            store,
            hooks,
            running: Mutex::new(BTreeSet::new()),
        })
    }

    /// Almacén.
    pub fn store(&self) -> &Arc<WorkflowStore> {
        &self.store
    }

    /// Ganchos del host.
    pub fn hooks(&self) -> &Hooks {
        &self.hooks
    }

    /// `true` si ese workflow se está ejecutando.
    pub fn is_running(&self, id: &str) -> bool {
        self.running.lock().is_ok_and(|r| r.contains(id))
    }

    /// Ejecuta en segundo plano. `false` si ya se estaba ejecutando.
    pub fn start(self: &Arc<Self>, wf: Workflow) -> bool {
        {
            let mut r = self
                .running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !r.insert(wf.id.clone()) {
                return false;
            }
        }
        let me = Arc::clone(self);
        let id = wf.id.clone();
        let spawned = std::thread::Builder::new()
            .name("rukes-workflow".into())
            .spawn(move || {
                let report = me.run_now(&wf);
                me.finish(&wf, report);
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "no se pudo ejecutar el workflow");
            self.running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id);
            return false;
        }
        true
    }

    fn finish(&self, wf: &Workflow, report: Vec<StepReport>) {
        let failed: Vec<&StepReport> = report.iter().filter(|r| !r.ok).collect();
        tracing::info!(
            workflow = %wf.id,
            steps = report.len(),
            failed = failed.len(),
            "workflow terminado"
        );
        let body = if failed.is_empty() {
            trf("workflows.done_ok", &[("n", &report.len())])
        } else {
            let lines: Vec<String> = failed
                .iter()
                .map(|r| format!("✗ {}: {}", r.label, r.detail))
                .collect();
            format!(
                "{}\n{}",
                trf(
                    "workflows.done_failed",
                    &[("failed", &failed.len()), ("n", &report.len())]
                ),
                lines.join("\n")
            )
        };
        (self.hooks.notify)(&wf.name, &body, !failed.is_empty());
        self.store.set_report(&wf.id, report);
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&wf.id);
    }

    /// Ejecuta todos los pasos (bloqueante). Un paso fallido no detiene los siguientes.
    pub fn run_now(&self, wf: &Workflow) -> Vec<StepReport> {
        wf.steps
            .iter()
            .map(|s| {
                let label = s.label();
                let (ok, detail) = match self.step(s) {
                    Ok(d) => (true, d),
                    Err(e) => (false, e),
                };
                if !ok {
                    tracing::warn!(workflow = %wf.id, paso = %label, error = %detail, "paso fallido");
                }
                StepReport { label, ok, detail }
            })
            .collect()
    }

    fn step(&self, s: &Step) -> Result<String, String> {
        match &s.action {
            Action::App {
                target,
                args,
                cwd,
                admin,
            } => {
                let target = expand_env(target);
                let args = args.as_deref().map(expand_env);
                let cwd = cwd
                    .as_deref()
                    .map(|c| std::path::PathBuf::from(expand_env(c)));
                let before = winlist::list();
                let pid = if *admin {
                    rukes_platform::shell::launch(
                        &target,
                        args.as_deref(),
                        cwd.as_deref(),
                        rukes_platform::shell::Verb::RunAs,
                    )
                    .map(|()| None)
                } else {
                    rukes_platform::shell::launch_pid(&target, args.as_deref(), cwd.as_deref())
                }
                .map_err(|e| e.to_string())?;
                match &s.window {
                    Some(w) => self.place(w, &before, pid, &stem(&target), false),
                    None => Ok(String::new()),
                }
            }
            Action::Url(u) => {
                let before = winlist::list();
                rukes_platform::shell::launch(u, None, None, rukes_platform::shell::Verb::Open)
                    .map_err(|e| e.to_string())?;
                match &s.window {
                    Some(w) => self.place(w, &before, None, "", true),
                    None => Ok(String::new()),
                }
            }
            Action::Script { name, args } => {
                (self.hooks.run_script)(name, args).map(|()| tr("workflows.started").into())
            }
            Action::Wait(d) => {
                std::thread::sleep(*d);
                Ok(String::new())
            }
            Action::Theme(t) => (self.hooks.set_theme)(t).map(|()| String::new()),
            Action::PowerPlan(p) => {
                let arg = power_plan_arg(p);
                run_hidden("powercfg", &["/setactive", &arg]).map(|()| arg)
            }
            Action::Kill(names) => {
                let wanted: Vec<String> = names
                    .iter()
                    .map(|n| {
                        let n = n.trim().to_lowercase();
                        if n.ends_with(".exe") {
                            n
                        } else {
                            format!("{n}.exe")
                        }
                    })
                    .collect();
                let procs = rukes_platform::process::list().map_err(|e| e.to_string())?;
                let mut killed = 0;
                let mut errors = Vec::new();
                for p in procs
                    .iter()
                    .filter(|p| wanted.contains(&p.name.to_lowercase()))
                {
                    if p.is_protected() {
                        errors.push(trf("workflows.protected", &[("name", &p.name)]));
                        continue;
                    }
                    match rukes_platform::process::kill(p.pid) {
                        Ok(()) => killed += 1,
                        Err(e) => errors.push(format!("{} ({}): {e}", p.name, p.pid)),
                    }
                }
                if errors.is_empty() {
                    Ok(trf("workflows.killed", &[("n", &killed)]))
                } else {
                    Err(errors.join("; "))
                }
            }
            Action::Notify(text) => {
                (self.hooks.notify)("Rukes", text, false);
                Ok(String::new())
            }
        }
    }

    /// Espera la ventana del paso y la coloca.
    fn place(
        &self,
        w: &super::WindowDef,
        before: &[WindowInfo],
        pid: Option<u32>,
        target_stem: &str,
        is_url: bool,
    ) -> Result<String, String> {
        let (mon, warn) = monitor_for(w.monitor).ok_or_else(|| tr("workflows.no_monitors"))?;
        let (rect, maximize) = layout(mon.work, w)?;
        let wait = w
            .wait
            .as_deref()
            .and_then(super::parse_duration)
            .unwrap_or(if is_url { URL_WAIT } else { APP_WAIT });
        let known: BTreeSet<Hwnd> = before.iter().map(|x| x.hwnd).collect();
        let deadline = Instant::now() + wait;
        let mut found = None;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let all = winlist::list();
            let fresh: Vec<WindowInfo> = all
                .iter()
                .filter(|x| !known.contains(&x.hwnd))
                .cloned()
                .collect();
            let only_fresh = Instant::now() < deadline;
            let candidate = if only_fresh {
                pick(&fresh, &[], pid, target_stem, w.title.as_deref(), false)
            } else {
                pick(&fresh, &all, pid, target_stem, w.title.as_deref(), is_url)
            };
            if let Some(h) = candidate {
                found = Some(h);
                break;
            }
            if !only_fresh {
                break;
            }
        }
        let hwnd = found.ok_or_else(|| tr("workflows.no_window"))?;
        // Muchas apps se colocan solas al terminar de abrir: se espera un poco y se repite.
        std::thread::sleep(Duration::from_millis(400));
        if !monitor::place(hwnd, rect, maximize) {
            return Err(tr("workflows.no_window").into());
        }
        std::thread::sleep(Duration::from_millis(600));
        monitor::place(hwnd, rect, maximize);
        let title = winlist::list()
            .into_iter()
            .find(|x| x.hwnd == hwnd)
            .map(|x| x.title)
            .unwrap_or_default();
        let mut detail = crate::util::ellipsis(&title, 60);
        if let Some(wn) = warn {
            detail = format!("{detail} · {wn}");
        }
        Ok(detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn info(hwnd: Hwnd, pid: u32, title: &str, exe: &str) -> WindowInfo {
        WindowInfo {
            hwnd,
            title: title.into(),
            pid,
            exe: Some(PathBuf::from(exe)),
            minimized: false,
        }
    }

    #[test]
    fn picks_the_right_window() {
        let fresh = vec![
            info(1, 10, "Sin título - Bloc de notas", "C:/w/notepad.exe"),
            info(2, 20, "proyecto - Visual Studio Code", "C:/x/Code.exe"),
        ];
        let all = vec![info(3, 30, "docs.rs - Chrome", "C:/c/chrome.exe")];
        assert_eq!(pick(&fresh, &all, Some(20), "", None, false), Some(2));
        assert_eq!(pick(&fresh, &all, None, "code", None, false), Some(2));
        assert_eq!(pick(&fresh, &all, None, "", Some("bloc"), false), Some(1));
        assert_eq!(pick(&fresh, &all, None, "x", None, false), Some(1));
        assert_eq!(pick(&[], &all, None, "", Some("CHROME"), false), Some(3));
        assert_eq!(pick(&[], &all, None, "", None, true), Some(3));
        assert_eq!(pick(&[], &all, None, "", None, false), None);
        assert_eq!(pick(&fresh, &[], None, "", Some("nada"), false), None);
    }

    #[test]
    fn expands_env_and_plans() {
        // SAFETY de test: variable propia del proceso de pruebas.
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        assert_eq!(expand_env("~/x"), format!("{home}/x"));
        assert_eq!(expand_env("%USERPROFILE%\\a"), format!("{home}\\a"));
        assert_eq!(expand_env("100% seguro"), "100% seguro");
        assert_eq!(expand_env("%NO_EXISTE_RUKES%"), "%NO_EXISTE_RUKES%");
        assert_eq!(expand_env("a~b"), "a~b");
        assert_eq!(power_plan_arg("Alto rendimiento"), "SCHEME_MIN");
        assert_eq!(power_plan_arg("balanced"), "SCHEME_BALANCED");
        assert_eq!(power_plan_arg("power_saver"), "SCHEME_MAX");
        assert_eq!(
            power_plan_arg("381b4222-f694-41f0-9685-ff5bb260df2e"),
            "381b4222-f694-41f0-9685-ff5bb260df2e"
        );
        assert_eq!(stem("C:/Program Files/X/Code.exe"), "code");
        assert_eq!(stem("wt.exe"), "wt");
    }

    #[test]
    fn runs_steps_and_reports_each_one() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = std::env::temp_dir().join(format!("rukes-wfrun-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("t.toml"),
            r#"
[workflow.t]
name = "Prueba"
steps = [
  { notify = "hola" },
  { wait = "50ms" },
  { script = "s1", args = "a b" },
  { theme = "nord" },
  { theme = "no-existe" },
  { kill = "rukes-proceso-que-no-existe" },
]
"#,
        )
        .unwrap();
        let (store, errors) = WorkflowStore::open(dir.clone());
        assert!(errors.is_empty(), "{errors:?}");
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        let (l1, l2, l3) = (Arc::clone(&log), Arc::clone(&log), Arc::clone(&log));
        let hooks = Hooks {
            run_script: Arc::new(move |n, a| {
                l1.lock()
                    .unwrap()
                    .push(format!("script {n} {}", a.join(",")));
                Ok(())
            }),
            set_theme: Arc::new(move |t| {
                l2.lock().unwrap().push(format!("theme {t}"));
                if t == "nord" {
                    Ok(())
                } else {
                    Err("tema desconocido".into())
                }
            }),
            notify: Arc::new(move |t, b, e| {
                l3.lock().unwrap().push(format!("notify {t}|{b}|{e}"));
            }),
        };
        let runner = Runner::new(Arc::clone(&store), hooks);
        let wf = store.find("t").unwrap();
        let report = runner.run_now(&wf);
        assert_eq!(report.len(), 6);
        assert!(
            report.iter().enumerate().all(|(i, r)| r.ok == (i != 4)),
            "{report:?}"
        );
        assert_eq!(report[5].detail, "0 procesos cerrados");
        let log = log.lock().unwrap().clone();
        assert_eq!(
            log,
            [
                "notify Rukes|hola|false",
                "script s1 a,b",
                "theme nord",
                "theme no-existe"
            ]
        );
        // En segundo plano: no se ejecuta dos veces a la vez y deja el informe.
        assert!(runner.start(wf.clone()));
        let t0 = Instant::now();
        while runner.is_running("t") && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let (_, rep) = store.report("t").unwrap();
        assert_eq!(rep.len(), 6);
        let _ = std::fs::remove_dir_all(dir);
    }
}
