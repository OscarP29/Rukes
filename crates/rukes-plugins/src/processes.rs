//! Procesos con RAM y CPU; terminar con confirmación (RF-051, RF-052).
//!
//! `kill chrome` lista el grupo `chrome.exe` (todos sus procesos) y cada proceso por separado,
//! ordenados por memoria. Enter pide confirmación; desde ahí se puede terminar también el árbol.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rukes_core::i18n::{Lang, language};
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata, Query,
    ResultItem,
};
use rukes_platform::process::{self, ProcessInfo};

/// Máximo de procesos individuales listados.
const MAX_SINGLE: usize = 40;

/// Muestra de tiempos de CPU para calcular porcentajes.
struct Sample {
    at: Instant,
    cpu: HashMap<u32, u64>,
}

/// Plugin de procesos.
#[derive(Default)]
pub struct ProcessesPlugin {
    last: Mutex<Option<Sample>>,
    /// Prioridad configurada.
    pub priority: f32,
}

fn es() -> bool {
    language() == Lang::Es
}

/// Formatea bytes en MB/GB.
pub fn fmt_bytes(b: u64) -> String {
    let mb = b as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// Grupo de procesos con el mismo ejecutable.
#[derive(Debug, Clone)]
pub struct Group<'a> {
    /// Nombre (`chrome.exe`).
    pub name: &'a str,
    /// Procesos.
    pub procs: Vec<&'a ProcessInfo>,
    /// Memoria total.
    pub memory: u64,
    /// CPU total en %.
    pub cpu: f32,
}

/// Agrupa por nombre y filtra por la consulta. Devuelve los grupos con su puntuación.
pub fn groups<'a>(
    all: &'a [ProcessInfo],
    cpu: &HashMap<u32, f32>,
    text: &str,
) -> Vec<(Group<'a>, f32)> {
    let mut by_name: HashMap<String, Group<'a>> = HashMap::new();
    for p in all.iter().filter(|p| p.pid > 4) {
        let g = by_name
            .entry(p.name.to_ascii_lowercase())
            .or_insert_with(|| Group {
                name: &p.name,
                procs: Vec::new(),
                memory: 0,
                cpu: 0.0,
            });
        g.memory += p.memory;
        g.cpu += cpu.get(&p.pid).copied().unwrap_or(0.0);
        g.procs.push(p);
    }
    let mut f = Fuzzy::new(text);
    let mut out: Vec<(Group<'a>, f32)> = by_name
        .into_values()
        .filter_map(|mut g| {
            let stem = g.name.strip_suffix(".exe").unwrap_or(g.name);
            let score = if text.is_empty() {
                Some(1.0)
            } else {
                f.score(stem)
            }?;
            g.procs.sort_by(|a, b| b.memory.cmp(&a.memory));
            Some((g, score))
        })
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.0.memory.cmp(&a.0.memory)));
    out
}

fn payload(raw: &str, pids: &[u32], name: &str) -> String {
    let pids: Vec<String> = pids.iter().map(u32::to_string).collect();
    format!("{name}|{}|{raw}", pids.join(","))
}

fn parse_payload(p: &str) -> Option<(&str, Vec<u32>, &str)> {
    let mut it = p.splitn(3, '|');
    let name = it.next()?;
    let pids = it
        .next()?
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    Some((name, pids, it.next().unwrap_or("")))
}

impl ProcessesPlugin {
    /// Crea el plugin con la prioridad configurada.
    pub fn new(priority: f32) -> Self {
        Self {
            last: Mutex::new(None),
            priority,
        }
    }

    /// Lista los procesos y su CPU (%) desde la muestra anterior. Si no hay una reciente, toma
    /// dos muestras separadas 150 ms.
    fn snapshot(&self, cancel: &CancelToken) -> Option<(Vec<ProcessInfo>, HashMap<u32, f32>)> {
        let mut prev = self.last.lock().unwrap_or_else(|p| p.into_inner()).take();
        if prev
            .as_ref()
            .is_none_or(|s| s.at.elapsed() > Duration::from_secs(10))
        {
            let first = process::list().ok()?;
            prev = Some(Sample {
                at: Instant::now(),
                cpu: first.iter().map(|p| (p.pid, p.cpu_time)).collect(),
            });
            std::thread::sleep(Duration::from_millis(150));
        }
        if cancel.is_cancelled() {
            *self.last.lock().unwrap_or_else(|p| p.into_inner()) = prev;
            return None;
        }
        let all = process::list().ok()?;
        let now = Instant::now();
        let mut cpu = HashMap::new();
        if let Some(prev) = &prev {
            let wall = now.duration_since(prev.at).as_secs_f64() * 1e7; // en 100 ns
            let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
            if wall > 0.0 {
                for p in &all {
                    if let Some(&before) = prev.cpu.get(&p.pid) {
                        let d = p.cpu_time.saturating_sub(before) as f64;
                        cpu.insert(p.pid, (d / wall / cores * 100.0) as f32);
                    }
                }
            }
        }
        // La muestra se renueva solo si la anterior tiene más de 1 s (si no, el % sale ruidoso).
        let keep_prev = prev
            .as_ref()
            .is_some_and(|s| s.at.elapsed() < Duration::from_secs(1));
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = if keep_prev {
            prev
        } else {
            Some(Sample {
                at: now,
                cpu: all.iter().map(|p| (p.pid, p.cpu_time)).collect(),
            })
        };
        Some((all, cpu))
    }

    fn items(all: &[ProcessInfo], cpu: &HashMap<u32, f32>, q: &Query) -> Vec<ResultItem> {
        let found = groups(all, cpu, &q.text);
        let mut out = Vec::new();
        let total = found.len();
        for (rank, (g, score)) in found.iter().enumerate() {
            let protected = g.procs.iter().any(|p| p.is_protected());
            let exe = g.procs.iter().find_map(|p| p.exe.clone());
            let count = g.procs.len();
            let mut it = ResultItem::new(
                format!("group:{}", g.name.to_ascii_lowercase()),
                if count > 1 {
                    format!("{} ×{count}", g.name)
                } else {
                    g.name.to_owned()
                },
            );
            let sys = if es() {
                "proceso del sistema"
            } else {
                "system process"
            };
            let what = if count > 1 {
                if es() {
                    format!("{count} procesos")
                } else {
                    format!("{count} processes")
                }
            } else {
                format!("PID {}", g.procs[0].pid)
            };
            it.subtitle = format!(
                "{} · CPU {:.1} % · {what}{}",
                fmt_bytes(g.memory),
                g.cpu,
                if protected {
                    format!(" · {sys}")
                } else {
                    String::new()
                }
            );
            it.icon = exe
                .clone()
                .map_or(Icon::Named("process".into()), Icon::File);
            // Sin texto (`ps`): orden por memoria.
            it.fuzzy = if q.text.is_empty() {
                1.0 - rank as f32 / (total as f32 + 1.0)
            } else {
                *score
            };
            if protected {
                it.actions.clear();
            }
            let pids: Vec<u32> = g.procs.iter().map(|p| p.pid).collect();
            it.payload = payload(&q.raw, &pids, g.name);
            out.push(it);
        }
        // Procesos sueltos del grupo que mejor coincide (para «cerrar el que elija»).
        if !q.text.is_empty()
            && let Some((g, score)) = found.first()
            && g.procs.len() > 1
        {
            for (i, p) in g.procs.iter().take(MAX_SINGLE).enumerate() {
                let mut it = ResultItem::new(
                    format!("pid:{}", p.pid),
                    format!("{} · PID {}", p.name, p.pid),
                );
                it.subtitle = format!(
                    "{} · CPU {:.1} %",
                    fmt_bytes(p.memory),
                    cpu.get(&p.pid).copied().unwrap_or(0.0)
                );
                it.icon = p
                    .exe
                    .clone()
                    .map_or(Icon::Named("process".into()), Icon::File);
                it.fuzzy = score * 0.98 - i as f32 * 0.001;
                if p.is_protected() {
                    it.actions.clear();
                }
                it.payload = payload(&q.raw, &[p.pid], &p.name);
                out.push(it);
            }
        }
        out
    }

    fn confirmation(item: &ResultItem) -> Vec<ResultItem> {
        let Some((name, pids, _)) = parse_payload(&item.payload) else {
            return Vec::new();
        };
        let n = pids.len();
        let target = if n > 1 {
            if es() {
                format!("{name} ({n} procesos)")
            } else {
                format!("{name} ({n} processes)")
            }
        } else {
            format!("{name} (PID {})", pids.first().copied().unwrap_or(0))
        };
        let mk = |id: &str, title: String, sub: &str| {
            let mut it = ResultItem::new(id, title);
            it.subtitle = sub.to_owned();
            it.icon = Icon::Named("process".into());
            it.payload = item.payload.clone();
            it
        };
        vec![
            mk(
                "confirm:kill",
                if es() {
                    format!("Sí, terminar {target}")
                } else {
                    format!("Yes, end {target}")
                },
                if es() {
                    "Se perderán los datos sin guardar"
                } else {
                    "Unsaved data will be lost"
                },
            ),
            mk(
                "confirm:tree",
                if es() {
                    format!("Terminar {target} y sus procesos hijos")
                } else {
                    format!("End {target} and its child processes")
                },
                if es() {
                    "Árbol de procesos"
                } else {
                    "Process tree"
                },
            ),
            mk(
                "cancel",
                if es() {
                    "Cancelar".into()
                } else {
                    "Cancel".into()
                },
                "",
            ),
        ]
    }

    fn kill(payload: &str, tree: bool) -> Result<String, PluginError> {
        let (name, pids, raw) =
            parse_payload(payload).ok_or_else(|| PluginError::Failed("payload".into()))?;
        let all = process::list().map_err(|e| PluginError::Failed(e.to_string()))?;
        let mut targets: Vec<u32> = Vec::new();
        for pid in pids {
            // El PID pudo reutilizarse: solo si sigue siendo el mismo ejecutable.
            let Some(p) = all
                .iter()
                .find(|p| p.pid == pid && p.name.eq_ignore_ascii_case(name))
            else {
                continue;
            };
            if p.is_protected() {
                continue;
            }
            if tree {
                targets.extend(process::descendants(&all, pid));
            }
            targets.push(pid);
        }
        let mut seen = std::collections::HashSet::new();
        let mut errors = 0;
        for pid in targets {
            if seen.insert(pid) && process::kill(pid).is_err() {
                errors += 1;
            }
        }
        if errors > 0 && errors == seen.len() {
            return Err(PluginError::Failed(
                if es() {
                    "No se pudo terminar (¿requiere administrador?)"
                } else {
                    "Could not end it (requires administrator?)"
                }
                .into(),
            ));
        }
        Ok(raw.to_owned())
    }
}

impl Plugin for ProcessesPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "processes".into(),
            name: if es() {
                "Procesos".into()
            } else {
                "Processes".into()
            },
            prefix: None,
            keywords: vec!["kill".into(), "ps".into(), "proc".into()],
            priority: 0.3 + self.priority,
            global: false,
        }
    }

    fn query(&self, q: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        if let Some((all, cpu)) = self.snapshot(cancel) {
            sink(Self::items(&all, &cpu, q));
        }
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if action != ActionId::OPEN {
            return Err(PluginError::UnsupportedAction(action.into()));
        }
        match item.id.as_str() {
            "cancel" => Ok(ActionOutcome::ReplaceQuery(
                parse_payload(&item.payload)
                    .map(|p| p.2.to_owned())
                    .unwrap_or_default(),
            )),
            "confirm:kill" => Self::kill(&item.payload, false).map(ActionOutcome::ReplaceQuery),
            "confirm:tree" => Self::kill(&item.payload, true).map(ActionOutcome::ReplaceQuery),
            _ => Ok(ActionOutcome::ShowResults(Self::confirmation(item))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rukes_core::QueryId;

    fn p(pid: u32, name: &str, mb: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent: 1,
            name: name.into(),
            exe: None,
            memory: mb << 20,
            cpu_time: 0,
        }
    }

    fn q(raw: &str, text: &str) -> Query {
        Query {
            id: QueryId(1),
            raw: raw.into(),
            text: text.into(),
            targeted: true,
        }
    }

    #[test]
    fn use_case_6_kill_chrome_lists_group_and_processes() {
        rukes_core::i18n::set_language("es", "");
        let all = [
            p(10, "chrome.exe", 300),
            p(11, "chrome.exe", 500),
            p(20, "Code.exe", 900),
            p(30, "csrss.exe", 5),
        ];
        let items = ProcessesPlugin::items(&all, &HashMap::new(), &q("kill chrome", "chrome"));
        assert_eq!(items[0].title, "chrome.exe ×2");
        assert!(
            items[0].subtitle.starts_with("800 MB"),
            "{}",
            items[0].subtitle
        );
        // Después, cada proceso por memoria.
        assert_eq!(items[1].id, "pid:11");
        assert_eq!(items[2].id, "pid:10");
        assert!(items.iter().all(|i| !i.title.contains("Code")));
    }

    #[test]
    fn enter_asks_for_confirmation_and_cancel_restores_query() {
        let all = [p(10, "chrome.exe", 300)];
        let items = ProcessesPlugin::items(&all, &HashMap::new(), &q("kill chrome", "chrome"));
        let plugin = ProcessesPlugin::default();
        let ActionOutcome::ShowResults(confirm) =
            plugin.execute(&items[0], ActionId::OPEN).unwrap()
        else {
            panic!("se esperaba confirmación");
        };
        assert_eq!(
            confirm.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["confirm:kill", "confirm:tree", "cancel"]
        );
        let ActionOutcome::ReplaceQuery(back) =
            plugin.execute(&confirm[2], ActionId::OPEN).unwrap()
        else {
            panic!("se esperaba volver a la consulta");
        };
        assert_eq!(back, "kill chrome");
    }

    #[test]
    fn protected_processes_have_no_actions() {
        let all = [p(30, "csrss.exe", 5)];
        let items = ProcessesPlugin::items(&all, &HashMap::new(), &q("kill csrss", "csrss"));
        assert!(items[0].actions.is_empty());
    }

    #[test]
    fn kills_a_real_child_process_after_confirmation() {
        let mut child = std::process::Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let payload = payload("kill ping", &[child.id()], "PING.EXE");
        let back = ProcessesPlugin::kill(&payload, false).unwrap();
        assert_eq!(back, "kill ping");
        assert_eq!(child.wait().unwrap().code(), Some(1));
    }
}
