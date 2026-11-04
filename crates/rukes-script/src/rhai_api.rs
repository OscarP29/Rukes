//! Scripts `.rhai` embebidos con una API segura (RF-139).
//!
//! El motor no puede leer ni escribir archivos ni importar módulos: solo tiene las funciones
//! registradas aquí. Cada ejecución tiene tiempo máximo y se puede cancelar: el motor comprueba
//! ambas cosas mientras avanza y también dentro de `sleep`, `run` y `http`.

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, Map, Position, Scope};

use crate::filter::FilterItem;
use crate::proc::{self, Exit, JobSlot};

/// Comprobación de cancelación.
pub type Cancel = Arc<dyn Fn() -> bool + Send + Sync>;

/// Notificación: `(título, texto, es_error)`.
pub type Notifier = Arc<dyn Fn(&str, &str, bool) + Send + Sync>;

/// Entrada de una ejecución.
pub struct Input<'a> {
    /// Código fuente.
    pub source: &'a str,
    /// Nombre del script (título de las notificaciones).
    pub name: &'a str,
    /// Texto escrito tras el alias.
    pub query: &'a str,
    /// Argumentos `(nombre, valor)`.
    pub args: &'a [(String, String)],
    /// Variables `RUKES_*` de esta ejecución (las lee `env()`).
    pub env: &'a [(String, String)],
    /// Momento límite.
    pub deadline: Option<Instant>,
    /// Devuelve `true` si hay que parar.
    pub cancel: Cancel,
    /// Notificaciones.
    pub notify: Notifier,
    /// Job del proceso que lance `run`, para cancelarlo.
    pub job: Arc<JobSlot>,
}

/// Resultado.
#[derive(Debug, Clone)]
pub struct Output {
    /// Fin.
    pub exit: Exit,
    /// Lo escrito con `print` y el valor final si es texto.
    pub stdout: String,
    /// Error.
    pub stderr: String,
    /// Resultados si el valor final es una lista de mapas.
    pub items: Option<Vec<FilterItem>>,
}

/// Tiempo máximo de una petición HTTP aunque al script le quede más.
const HTTP_MAX: Duration = Duration::from_secs(30);

fn err(msg: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        Dynamic::from(msg.into()),
        Position::NONE,
    ))
}

#[derive(Clone)]
struct Limits {
    deadline: Option<Instant>,
    cancel: Cancel,
}

impl Limits {
    fn stop(&self) -> bool {
        (self.cancel)() || self.deadline.is_some_and(|d| Instant::now() >= d)
    }

    fn remaining(&self, cap: Duration) -> Duration {
        self.deadline.map_or(cap, |d| {
            d.saturating_duration_since(Instant::now()).min(cap)
        })
    }

    fn check(&self) -> Result<(), Box<EvalAltResult>> {
        if self.stop() {
            Err(Box::new(EvalAltResult::ErrorTerminated(
                Dynamic::UNIT,
                Position::NONE,
            )))
        } else {
            Ok(())
        }
    }
}

fn http_call(
    limits: &Limits,
    method: &str,
    url: &str,
    body: &str,
    headers: Vec<(String, String)>,
) -> Result<Map, Box<EvalAltResult>> {
    limits.check()?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(err(format!("solo se admiten URLs http(s): {url}")));
    }
    let r = rukes_platform::http::send(&rukes_platform::http::Request {
        method,
        url,
        headers: &headers,
        body: body.as_bytes(),
        timeout: limits.remaining(HTTP_MAX).max(Duration::from_millis(100)),
        max_bytes: 8 * 1024 * 1024,
    })
    .map_err(|e| err(format!("HTTP {method} {url}: {e}")))?;
    let mut m = Map::new();
    m.insert("status".into(), Dynamic::from(i64::from(r.status)));
    m.insert(
        "body".into(),
        Dynamic::from(String::from_utf8_lossy(&r.body).into_owned()),
    );
    Ok(m)
}

fn find_window(pattern: &str) -> Option<rukes_platform::winlist::WindowInfo> {
    let p = pattern.to_lowercase();
    rukes_platform::winlist::list()
        .into_iter()
        .find(|w| w.title.to_lowercase().contains(&p))
}

fn engine(input: &Input<'_>, out: Arc<Mutex<String>>) -> Engine {
    let mut e = Engine::new();
    e.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
    e.disable_symbol("eval");
    e.set_max_call_levels(64);
    e.set_max_expr_depths(64, 32);
    e.set_max_string_size(16 * 1024 * 1024);
    e.set_max_array_size(1_000_000);
    e.set_max_map_size(100_000);
    let limits = Limits {
        deadline: input.deadline,
        cancel: Arc::clone(&input.cancel),
    };
    {
        let l = limits.clone();
        e.on_progress(move |ops| (ops % 512 == 0 && l.stop()).then_some(Dynamic::UNIT));
    }
    {
        let o = Arc::clone(&out);
        e.on_print(move |s| {
            let mut o = o.lock().unwrap_or_else(|p| p.into_inner());
            o.push_str(s);
            o.push('\n');
        });
    }
    {
        let o = Arc::clone(&out);
        e.on_debug(move |s, _, pos| {
            let mut o = o.lock().unwrap_or_else(|p| p.into_inner());
            o.push_str(&format!("[debug {pos}] {s}\n"));
        });
    }

    e.register_fn("open", |target: &str| -> Result<(), Box<EvalAltResult>> {
        rukes_platform::shell::launch(target, None, None, rukes_platform::shell::Verb::Open)
            .map_err(|x| err(format!("open «{target}»: {x}")))
    });

    let run_impl = {
        let l = limits.clone();
        let job = Arc::clone(&input.job);
        move |program: &str, args: Vec<String>| -> Result<Map, Box<EvalAltResult>> {
            l.check()?;
            let exe = crate::lang::find_in_path(program)
                .unwrap_or_else(|| std::path::PathBuf::from(program));
            let mut cmd = Command::new(exe);
            cmd.args(&args);
            let cancel = Arc::clone(&l.cancel);
            let r = proc::run_captured(cmd, l.deadline, &|| cancel(), Some(&job));
            let code = match r.exit {
                Exit::Code(c) => i64::from(c),
                Exit::Timeout | Exit::Cancelled => {
                    return Err(Box::new(EvalAltResult::ErrorTerminated(
                        Dynamic::UNIT,
                        Position::NONE,
                    )));
                }
                Exit::SpawnError(e) => return Err(err(format!("run «{program}»: {e}"))),
            };
            let mut m = Map::new();
            m.insert("code".into(), Dynamic::from(code));
            m.insert("stdout".into(), Dynamic::from(r.stdout));
            m.insert("stderr".into(), Dynamic::from(r.stderr));
            Ok(m)
        }
    };
    {
        let f = run_impl.clone();
        e.register_fn("run", move |program: &str| f(program, Vec::new()));
    }
    e.register_fn("run", move |program: &str, args: Array| {
        let args = args.into_iter().map(|a| a.to_string()).collect();
        run_impl(program, args)
    });

    e.register_fn("clipboard", || -> String {
        rukes_platform::clipboard::get_text(16 * 1024 * 1024).unwrap_or_default()
    });
    e.register_fn(
        "set_clipboard",
        |text: &str| -> Result<(), Box<EvalAltResult>> {
            rukes_platform::clipboard::set_text(text).map_err(|x| err(x.to_string()))
        },
    );

    {
        let n = Arc::clone(&input.notify);
        let title = input.name.to_owned();
        e.register_fn("notify", move |text: &str| n(&title, text, false));
    }
    {
        let n = Arc::clone(&input.notify);
        e.register_fn("notify", move |title: &str, text: &str| {
            n(title, text, false)
        });
    }

    {
        let l = limits.clone();
        e.register_fn("sleep", move |ms: i64| -> Result<(), Box<EvalAltResult>> {
            let end = Instant::now() + Duration::from_millis(ms.max(0).unsigned_abs());
            while Instant::now() < end {
                l.check()?;
                std::thread::sleep(
                    end.saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(20)),
                );
            }
            l.check()
        });
    }

    {
        let l = limits.clone();
        e.register_fn(
            "http_get",
            move |url: &str| -> Result<String, Box<EvalAltResult>> {
                let m = http_call(&l, "GET", url, "", Vec::new())?;
                Ok(m.get("body").map(Dynamic::to_string).unwrap_or_default())
            },
        );
    }
    {
        let l = limits.clone();
        e.register_fn(
            "http_post",
            move |url: &str, body: &str| -> Result<String, Box<EvalAltResult>> {
                let m = http_call(&l, "POST", url, body, Vec::new())?;
                Ok(m.get("body").map(Dynamic::to_string).unwrap_or_default())
            },
        );
    }
    {
        let l = limits.clone();
        e.register_fn("http", move |req: Map| -> Result<Map, Box<EvalAltResult>> {
            let get = |k: &str| req.get(k).map(Dynamic::to_string).unwrap_or_default();
            let method = req
                .get("method")
                .map_or_else(|| "GET".to_owned(), Dynamic::to_string);
            let headers = req
                .get("headers")
                .and_then(|h| h.read_lock::<Map>().map(|m| m.clone()))
                .map(|m| {
                    m.into_iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            http_call(&l, &method, &get("url"), &get("body"), headers)
        });
    }

    e.register_fn("windows", || -> Array {
        rukes_platform::winlist::list()
            .into_iter()
            .map(|w| {
                let mut m = Map::new();
                m.insert("title".into(), Dynamic::from(w.title));
                m.insert("pid".into(), Dynamic::from(i64::from(w.pid)));
                m.insert(
                    "exe".into(),
                    Dynamic::from(
                        w.exe
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    ),
                );
                m.insert("minimized".into(), Dynamic::from(w.minimized));
                Dynamic::from_map(m)
            })
            .collect()
    });
    e.register_fn("focus_window", |pattern: &str| -> bool {
        find_window(pattern).is_some_and(|w| rukes_platform::winlist::activate(w.hwnd))
    });
    e.register_fn(
        "move_window",
        |pattern: &str, x: i64, y: i64, w: i64, h: i64| -> bool {
            let clamp = |v: i64| i32::try_from(v).unwrap_or(0);
            find_window(pattern).is_some_and(|win| {
                rukes_platform::winlist::set_rect(win.hwnd, clamp(x), clamp(y), clamp(w), clamp(h))
            })
        },
    );

    {
        let env: Vec<(String, String)> = input.env.to_vec();
        e.register_fn("env", move |name: &str| -> String {
            env.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
                .or_else(|| std::env::var(name).ok())
                .unwrap_or_default()
        });
    }

    e.register_fn(
        "parse_json",
        |text: &str| -> Result<Dynamic, Box<EvalAltResult>> {
            let v: serde_json::Value =
                serde_json::from_str(text).map_err(|x| err(format!("JSON no válido: {x}")))?;
            rhai::serde::to_dynamic(v)
        },
    );
    e.register_fn(
        "to_json",
        |v: Dynamic| -> Result<String, Box<EvalAltResult>> {
            serde_json::to_string(&v).map_err(|x| err(x.to_string()))
        },
    );
    e
}

/// Ejecuta un script Rhai.
pub fn run(input: &Input<'_>) -> Output {
    let out = Arc::new(Mutex::new(String::new()));
    let engine = engine(input, Arc::clone(&out));
    let mut scope = Scope::new();
    let mut args = Map::new();
    for (k, v) in input.args {
        args.insert(k.as_str().into(), Dynamic::from(v.clone()));
    }
    let var = |name: &str| {
        input
            .env
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    scope.push_constant("query", ImmutableString::from(input.query));
    scope.push_constant("args", args);
    scope.push_constant("selected_file", var("RUKES_SELECTED_FILE"));
    scope.push_constant("active_window", var("RUKES_ACTIVE_WINDOW"));
    let result = engine.eval_with_scope::<Dynamic>(&mut scope, input.source);
    let mut stdout = out.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let mut items = None;
    let exit = match result {
        Ok(v) => {
            if v.is_array() {
                match rhai::serde::from_dynamic::<Vec<FilterItem>>(&v) {
                    Ok(list) => items = Some(list),
                    Err(_) => stdout.push_str(&v.to_string()),
                }
            } else if !v.is_unit() {
                stdout.push_str(&v.to_string());
            }
            Exit::Code(0)
        }
        Err(e) => match *e {
            EvalAltResult::ErrorTerminated(..) => {
                if (input.cancel)() {
                    Exit::Cancelled
                } else {
                    Exit::Timeout
                }
            }
            other => {
                return Output {
                    exit: Exit::Code(1),
                    stdout,
                    stderr: other.to_string(),
                    items: None,
                };
            }
        },
    };
    Output {
        exit,
        stdout,
        stderr: String::new(),
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn input<'a>(source: &'a str, deadline: Option<Instant>, notify: Notifier) -> Input<'a> {
        Input {
            source,
            name: "test",
            query: "q",
            args: &[],
            env: &[],
            deadline,
            cancel: Arc::new(|| false),
            notify,
            job: Arc::new(JobSlot::default()),
        }
    }

    fn quiet() -> Notifier {
        Arc::new(|_, _, _| {})
    }

    #[test]
    fn print_and_return_value() {
        let r = run(&input("print(\"a\"); 1 + 2", None, quiet()));
        assert_eq!(r.exit, Exit::Code(0));
        assert_eq!(r.stdout, "a\n3");
    }

    #[test]
    fn args_query_and_env() {
        let args = [("title".to_owned(), "Mi bug".to_owned())];
        let env = [("RUKES_ACTIVE_WINDOW".to_owned(), "Notas".to_owned())];
        let mut i = input(
            "`${args.title}|${query}|${active_window}|${env(\"RUKES_ACTIVE_WINDOW\")}`",
            None,
            quiet(),
        );
        i.args = &args;
        i.env = &env;
        assert_eq!(run(&i).stdout, "Mi bug|q|Notas|Notas");
    }

    #[test]
    fn infinite_loop_is_stopped_by_the_deadline() {
        let t = Instant::now();
        let r = run(&input(
            "let x = 0; loop { x += 1; }",
            Some(Instant::now() + Duration::from_millis(200)),
            quiet(),
        ));
        assert_eq!(r.exit, Exit::Timeout);
        assert!(t.elapsed() < Duration::from_secs(2));
        // `sleep` también respeta el límite.
        let r = run(&input(
            "sleep(60000)",
            Some(Instant::now() + Duration::from_millis(100)),
            quiet(),
        ));
        assert_eq!(r.exit, Exit::Timeout);
    }

    #[test]
    fn cancel_flag_stops_it() {
        let flag = Arc::new(AtomicBool::new(false));
        let f = Arc::clone(&flag);
        let mut i = input("loop { sleep(10); }", None, quiet());
        i.cancel = Arc::new(move || f.load(Ordering::SeqCst));
        let c = flag;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            c.store(true, Ordering::SeqCst);
        });
        assert_eq!(run(&i).exit, Exit::Cancelled);
    }

    #[test]
    fn errors_and_sandbox() {
        let r = run(&input("throw \"falló\"", None, quiet()));
        assert_eq!(r.exit, Exit::Code(1));
        assert!(r.stderr.contains("falló"));
        // Sin módulos ni eval.
        assert_eq!(
            run(&input("import \"x\" as y;", None, quiet())).exit,
            Exit::Code(1)
        );
        assert_eq!(
            run(&input("eval(\"1\")", None, quiet())).exit,
            Exit::Code(1)
        );
        // Error de sintaxis.
        assert_eq!(run(&input("let = ;", None, quiet())).exit, Exit::Code(1));
    }

    #[test]
    fn run_notify_json_and_items() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = Arc::clone(&seen);
        let n: Notifier = Arc::new(move |t, b, _| s2.lock().unwrap().push(format!("{t}:{b}")));
        let r = run(&input(
            r#"
            let r = run("cmd", ["/d", "/c", "echo hola"]);
            notify("n1");
            notify("T", "n2");
            let j = parse_json("{\"a\": [1, 2]}");
            print(to_json(j.a));
            [#{ title: r.stdout, open: "https://x" }]
            "#,
            None,
            n,
        ));
        assert_eq!(r.exit, Exit::Code(0), "{}", r.stderr);
        assert_eq!(r.stdout, "[1,2]\n");
        let items = r.items.unwrap();
        assert_eq!(items[0].title.trim(), "hola");
        assert_eq!(items[0].open.as_deref(), Some("https://x"));
        assert_eq!(*seen.lock().unwrap(), ["test:n1", "T:n2"]);
    }

    #[test]
    fn http_rejects_other_schemes() {
        let r = run(&input(
            "http_get(\"file:///C:/Windows/win.ini\")",
            None,
            quiet(),
        ));
        assert_eq!(r.exit, Exit::Code(1));
        assert!(r.stderr.contains("http(s)"));
    }
}
