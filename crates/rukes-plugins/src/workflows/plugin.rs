//! Workflows en el launcher (RF-154, RF-155) y lista de triggers.

use std::sync::Arc;

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action as UiAction, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError,
    PluginMetadata, Query, ResultItem,
};

use super::{Runner, Scheduler, Workflow};
use crate::util::{ago, now_unix};

/// Plugin de workflows. `triggers` lista los triggers y su próxima ejecución.
pub struct WorkflowsPlugin {
    /// Ejecutor (con su almacén).
    pub runner: Arc<Runner>,
    /// Planificador, para mostrar si está en pausa.
    pub scheduler: Option<Arc<Scheduler>>,
}

fn when_from_now(unix: u64) -> String {
    let left = unix.saturating_sub(now_unix());
    let t = rukes_platform::time::local_from(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix),
    )
    .map(|t| format!("{:02}/{:02} {:02}:{:02}", t.day, t.month, t.hour, t.minute))
    .unwrap_or_default();
    let rel = super::human_duration(std::time::Duration::from_secs(left - left % 60));
    trf("workflows.next_run", &[("in", &rel), ("at", &t)])
}

impl WorkflowsPlugin {
    fn detail(&self, w: &Workflow) -> String {
        let mut out = String::new();
        if let Some(d) = &w.description {
            out.push_str(d);
            out.push_str("\n\n");
        }
        out.push_str(tr("workflows.steps_title"));
        out.push('\n');
        for (i, s) in w.steps.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, s.label()));
        }
        if let Some((when, report)) = self.runner.store().report(&w.id) {
            out.push('\n');
            out.push_str(&trf("workflows.last_run", &[("when", &ago(when))]));
            out.push('\n');
            for r in report {
                let mark = if r.ok { "✓" } else { "✗" };
                if r.detail.is_empty() {
                    out.push_str(&format!("{mark} {}\n", r.label));
                } else {
                    out.push_str(&format!("{mark} {} — {}\n", r.label, r.detail));
                }
            }
        }
        out
    }

    fn item(&self, w: &Workflow, score: f32, exact: bool) -> ResultItem {
        let mut it = ResultItem::new(format!("w:{}", w.id), w.name.clone());
        let mut sub = vec![trf("workflows.n_steps", &[("n", &w.steps.len())])];
        if let Some(d) = &w.description {
            sub.insert(0, d.clone());
        }
        if !w.alias.is_empty() {
            sub.push(w.alias.join(", "));
        }
        if let Some(h) = &w.hotkey {
            sub.push(h.clone());
        }
        if self.runner.is_running(&w.id) {
            sub.insert(0, tr("workflows.running").into());
        }
        it.subtitle = sub.join(" · ");
        it.icon = Icon::Named("workflow".into());
        it.fuzzy = score;
        it.alias_match = exact;
        it.detail = Some(self.detail(w));
        it.payload = w.id.clone();
        it.actions = vec![
            UiAction::new(ActionId::OPEN, tr("workflows.run")),
            UiAction::new(ActionId::EDIT, tr("workflows.edit")),
        ];
        it
    }

    fn triggers(&self, text: &str) -> Vec<ResultItem> {
        let store = self.runner.store();
        let paused = self.scheduler.as_ref().map(|s| s.paused()).unwrap_or(false);
        let mut out = Vec::new();
        let mut f = Fuzzy::new(text);
        for (i, t) in store.triggers().into_iter().enumerate() {
            if !text.is_empty() && f.score(&t.name).is_none() {
                continue;
            }
            let mut it = ResultItem::new(format!("t:{i}"), t.name.clone());
            let mut sub = vec![t.when_label()];
            if !t.days.is_empty() {
                let names: Vec<&str> = t
                    .days
                    .iter()
                    .map(|d| tr(&format!("workflows.day{d}")))
                    .collect();
                sub.push(names.join(" "));
            }
            sub.push(if !t.enabled {
                tr("workflows.disabled").into()
            } else if paused {
                tr("workflows.paused").into()
            } else {
                store
                    .next_run(&t)
                    .map_or_else(|| tr("workflows.no_next").into(), when_from_now)
            });
            it.subtitle = sub.join(" · ");
            it.icon = Icon::Named("trigger".into());
            it.fuzzy = 1.0 - i as f32 * 0.001;
            it.payload = format!("{:?}", t.target);
            it.actions = vec![UiAction::new(ActionId::EDIT, tr("workflows.edit"))];
            out.push(it);
        }
        if out.is_empty() {
            let mut it = ResultItem::new("t:none", tr("workflows.no_triggers"));
            it.subtitle = store.dir().display().to_string();
            it.icon = Icon::Named("trigger".into());
            it.actions = vec![UiAction::new(ActionId::OPEN, tr("workflows.open_dir"))];
            out.push(it);
        }
        out
    }

    /// Resultados de una consulta.
    pub fn search(&self, q: &Query) -> Vec<ResultItem> {
        let first = q.raw.split_whitespace().next().unwrap_or("").to_lowercase();
        if q.targeted && first == "triggers" {
            return self.triggers(q.text.trim());
        }
        let text = q.text.trim();
        let all = self.runner.store().workflows();
        if text.is_empty() {
            return if q.targeted {
                all.iter()
                    .enumerate()
                    .map(|(i, w)| self.item(w, 1.0 - i as f32 * 0.001, false))
                    .collect()
            } else {
                Vec::new()
            };
        }
        let lower = text.to_lowercase();
        let mut f = Fuzzy::new(text);
        all.iter()
            .filter_map(|w| {
                let exact = w.alias.contains(&lower) || w.name.to_lowercase() == lower;
                let score = if exact {
                    1.0
                } else {
                    f.best(
                        std::iter::once(w.name.as_str()).chain(w.alias.iter().map(String::as_str)),
                    )?
                };
                Some(self.item(w, score, exact))
            })
            .collect()
    }
}

impl Plugin for WorkflowsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "workflows".into(),
            name: tr("workflows.name").into(),
            prefix: None,
            keywords: vec!["workflow".into(), "workflows".into(), "triggers".into()],
            priority: 0.3,
            global: true,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(self.search(q));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let store = self.runner.store();
        let open_path = |p: &std::path::Path| {
            rukes_platform::shell::launch(
                &p.to_string_lossy(),
                None,
                None,
                rukes_platform::shell::Verb::Open,
            )
            .map(|()| ActionOutcome::Hide)
            .map_err(|e| PluginError::Failed(e.to_string()))
        };
        if item.id.starts_with("t:") {
            // Triggers: Ctrl+E o Enter abren la carpeta (cada archivo puede tener los suyos).
            return open_path(store.dir());
        }
        let wf = store
            .find(&item.payload)
            .ok_or_else(|| PluginError::Failed(tr("workflows.gone").into()))?;
        match action {
            ActionId::EDIT => open_path(&wf.path),
            ActionId::OPEN => {
                if self.runner.start(wf) {
                    Ok(ActionOutcome::Hide)
                } else {
                    Ok(ActionOutcome::Notify(
                        tr("workflows.already_running").into(),
                    ))
                }
            }
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Hooks, WorkflowStore};
    use super::*;
    use rukes_core::QueryId;

    #[test]
    fn finds_workflows_by_name_and_alias_and_lists_triggers() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = std::env::temp_dir().join(format!("rukes-wfp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.toml"),
            "[workflow.trabajo]\nname = \"Modo trabajo\"\nalias = [\"work\"]\nsteps = [{ wait = \"1s\" }, { notify = \"x\" }]\n\n[[trigger]]\nat = \"09:00\"\ndays = [\"laborables\"]\nrun = \"workflow:trabajo\"\n",
        )
        .unwrap();
        let (store, errors) = WorkflowStore::open(dir.clone());
        assert!(errors.is_empty(), "{errors:?}");
        let hooks = Hooks {
            run_script: Arc::new(|_, _| Ok(())),
            set_theme: Arc::new(|_| Ok(())),
            notify: Arc::new(|_, _, _| {}),
        };
        let p = WorkflowsPlugin {
            runner: Runner::new(store, hooks),
            scheduler: None,
        };
        let q = |raw: &str, text: &str, targeted: bool| Query {
            id: QueryId(1),
            raw: raw.into(),
            text: text.into(),
            targeted,
        };
        let r = p.search(&q("modo trabajo", "modo trabajo", false));
        assert_eq!(r[0].title, "Modo trabajo");
        assert!(r[0].alias_match);
        assert!(r[0].detail.as_deref().unwrap().contains("1. Esperar 1 s"));
        let r = p.search(&q("work", "work", false));
        assert!(r[0].alias_match);
        assert!(p.search(&q("zzz", "zzz", false)).is_empty());
        assert_eq!(p.search(&q("workflows", "", true)).len(), 1);
        let t = p.search(&q("triggers", "", true));
        assert_eq!(t[0].title, "workflow:trabajo");
        assert!(t[0].subtitle.contains("09:00"), "{}", t[0].subtitle);
        assert!(t[0].subtitle.contains("lun"), "{}", t[0].subtitle);
        let _ = std::fs::remove_dir_all(dir);
    }
}
