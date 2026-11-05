//! Triggers programados (RF-160) con pausa global (RF-163).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use super::{Trigger, When, WorkflowStore};
use crate::util::now_unix;

/// Día de la semana (0 = domingo) de una fecha del calendario gregoriano.
pub fn weekday(year: i32, month: u32, day: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let m = month.clamp(1, 12) as usize - 1;
    (y + y / 4 - y / 100 + y / 400 + T[m] + day as i32).rem_euclid(7) as u32
}

/// Margen tras arrancar Rukes antes de los triggers `startup`/`login`.
const STARTUP_DELAY: u64 = 5;
/// Rukes arrancado hasta estos segundos después de encender cuenta como inicio de sesión.
const LOGIN_WINDOW: u64 = 10 * 60;
/// Un trigger que debía saltar hace más de esto (equipo suspendido) se salta.
const MISSED_AFTER: u64 = 3600;

/// Próxima ejecución (unix) de `t` vista en `now` (unix), con la hora local `(h, m, s)`, el día
/// de la semana `wd` y los segundos desde que arrancó Windows. `None`: no vuelve a saltar.
pub fn next_due(
    t: &Trigger,
    now: u64,
    local: (u32, u32, u32),
    wd: u32,
    uptime: u64,
) -> Option<u64> {
    match t.when {
        When::Every(d) => Some(now + d.as_secs().max(60)),
        When::Startup => Some(now + STARTUP_DELAY),
        When::Login => (uptime < LOGIN_WINDOW).then_some(now + STARTUP_DELAY),
        When::At(h, m) => {
            let target = u64::from(h * 3600 + m * 60);
            let cur = u64::from(local.0 * 3600 + local.1 * 60 + local.2);
            let midnight = now - cur.min(now);
            let first = u64::from(target <= cur);
            (0..8).map(|k| first + k).find_map(|da| {
                let day = (wd + da as u32) % 7;
                (t.days.is_empty() || t.days.contains(&day))
                    .then_some(midnight + da * 86_400 + target)
            })
        }
    }
}

/// Planificador en su hilo. Al soltarse, el hilo termina.
pub struct Scheduler {
    wake: Sender<()>,
    paused: Arc<AtomicBool>,
}

impl Scheduler {
    /// Arranca el hilo. `fire` se llama en él con cada trigger que salta (no pausado).
    pub fn spawn(
        store: &Arc<WorkflowStore>,
        fire: Arc<dyn Fn(&Trigger) + Send + Sync>,
        paused: bool,
    ) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let paused = Arc::new(AtomicBool::new(paused));
        let weak = Arc::downgrade(store);
        let p = Arc::clone(&paused);
        let spawned = std::thread::Builder::new()
            .name("rukes-triggers".into())
            .spawn(move || run(&weak, &rx, &p, &*fire));
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "no se pudo arrancar el planificador de triggers");
        }
        Self { wake: tx, paused }
    }

    /// Pausa o reanuda todos los triggers (los que venzan en pausa se saltan).
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        tracing::info!(paused, "triggers");
        let _ = self.wake.send(());
    }

    /// En pausa.
    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Avisa de que cambiaron los triggers.
    pub fn reload(&self) {
        let _ = self.wake.send(());
    }
}

fn clock() -> ((u32, u32, u32), u32) {
    let t = rukes_platform::time::local_now();
    (
        (t.hour, t.minute, t.second),
        weekday(t.year, t.month, t.day),
    )
}

fn run(
    store: &Weak<WorkflowStore>,
    wake: &Receiver<()>,
    paused: &AtomicBool,
    fire: &(dyn Fn(&Trigger) + Send + Sync),
) {
    let mut due: BTreeMap<String, u64> = BTreeMap::new();
    // `startup`/`login` saltan una sola vez por arranque.
    let mut once_done: BTreeSet<String> = BTreeSet::new();
    let uptime = rukes_platform::sysinfo::uptime_secs();
    loop {
        let Some(s) = store.upgrade() else { return };
        let triggers: Vec<Trigger> = s.triggers().into_iter().filter(|t| t.enabled).collect();
        let now = now_unix();
        let (local, wd) = clock();
        let keys: BTreeSet<String> = triggers.iter().map(Trigger::key).collect();
        due.retain(|k, _| keys.contains(k));
        for t in &triggers {
            let k = t.key();
            if !due.contains_key(&k) && !once_done.contains(&k) {
                if let Some(d) = next_due(t, now, local, wd, uptime) {
                    due.insert(k, d);
                }
            }
        }
        for t in &triggers {
            let k = t.key();
            let Some(&d) = due.get(&k) else { continue };
            if d > now {
                continue;
            }
            if paused.load(Ordering::SeqCst) {
                tracing::info!(trigger = %t.name, "trigger saltado: en pausa");
            } else if now - d > MISSED_AFTER {
                tracing::info!(trigger = %t.name, "trigger saltado: hora perdida");
            } else {
                tracing::info!(trigger = %t.name, "trigger");
                fire(t);
            }
            match t.when {
                When::Startup | When::Login => {
                    due.remove(&k);
                    once_done.insert(k);
                }
                _ => match next_due(t, now_unix().max(d + 1), local, wd, uptime) {
                    Some(n) => {
                        due.insert(k, n);
                    }
                    None => {
                        due.remove(&k);
                    }
                },
            }
        }
        *super::lock(&s.next) = due.clone();
        drop(s);
        let now = now_unix();
        let wait = due
            .values()
            .map(|d| d.saturating_sub(now))
            .min()
            .unwrap_or(30)
            .clamp(1, 30);
        match wake.recv_timeout(Duration::from_secs(wait)) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Target;
    use super::*;

    fn trig(when: When, days: &[u32]) -> Trigger {
        Trigger {
            name: "t".into(),
            when,
            days: days.to_vec(),
            target: Target::Workflow("x".into()),
            enabled: true,
        }
    }

    #[test]
    fn weekdays() {
        assert_eq!(weekday(2026, 11, 4), 3); // miércoles
        assert_eq!(weekday(2024, 2, 29), 4); // jueves
        assert_eq!(weekday(2000, 1, 1), 6); // sábado
    }

    #[test]
    fn next_due_times() {
        // Miércoles 10:00:00; medianoche = 1_000_000 - 36_000.
        let now = 1_000_000;
        let mid = now - 36_000;
        let local = (10, 0, 0);
        assert_eq!(
            next_due(&trig(When::At(9, 0), &[]), now, local, 3, 0),
            Some(mid + 86_400 + 32_400)
        );
        assert_eq!(
            next_due(&trig(When::At(10, 30), &[]), now, local, 3, 0),
            Some(mid + 37_800)
        );
        assert_eq!(
            next_due(&trig(When::At(10, 0), &[]), now, local, 3, 0),
            Some(mid + 86_400 + 36_000)
        );
        // Solo lunes: el próximo es dentro de 5 días.
        assert_eq!(
            next_due(&trig(When::At(9, 0), &[1]), now, local, 3, 0),
            Some(mid + 5 * 86_400 + 32_400)
        );
        // Laborables a las 11:00 un miércoles: hoy.
        assert_eq!(
            next_due(&trig(When::At(11, 0), &[1, 2, 3, 4, 5]), now, local, 3, 0),
            Some(mid + 39_600)
        );
        assert_eq!(
            next_due(
                &trig(When::Every(Duration::from_secs(1800)), &[]),
                now,
                local,
                3,
                0
            ),
            Some(now + 1800)
        );
        assert_eq!(
            next_due(&trig(When::Login, &[]), now, local, 3, 60),
            Some(now + 5)
        );
        assert_eq!(next_due(&trig(When::Login, &[]), now, local, 3, 4000), None);
        assert_eq!(
            next_due(&trig(When::Startup, &[]), now, local, 3, 4000),
            Some(now + 5)
        );
    }

    #[test]
    fn startup_trigger_fires_once_and_pause_skips() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = std::env::temp_dir().join(format!("rukes-trig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("t.toml"),
            "[[trigger]]\non = \"startup\"\nrun = \"script:uno\"\n",
        )
        .unwrap();
        let (store, errors) = WorkflowStore::open(dir.clone());
        assert!(errors.is_empty(), "{errors:?}");
        let (tx, rx) = crossbeam_channel::unbounded::<String>();
        let sched = Scheduler::spawn(
            &store,
            Arc::new(move |t: &Trigger| {
                let _ = tx.send(format!("{:?}", t.target));
            }),
            false,
        );
        assert!(!sched.paused());
        let got = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(got, "Script(\"uno\")");
        sched.reload();
        assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err());
        sched.set_paused(true);
        assert!(sched.paused());
        drop(sched);
        let _ = std::fs::remove_dir_all(dir);
    }
}
