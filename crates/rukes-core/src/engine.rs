//! Motor de búsqueda: enruta la consulta a los plugins, los ejecuta en un pool de hilos con
//! cancelación y puntúa los resultados (ADR 0003).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{Receiver, Sender};

use crate::ranking::{self, ScoreInput};
use crate::{
    ActionOutcome, CancelToken, Frecency, Plugin, PluginError, PluginMetadata, Query, QueryId,
    ResultItem,
};

/// Ajustes del motor.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Peso de la frecency en la puntuación (0..1).
    pub frecency_weight: f32,
    /// Resultados máximos que se entregan por plugin y consulta.
    pub max_results_per_plugin: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            frecency_weight: 0.35,
            max_results_per_plugin: 50,
        }
    }
}

/// Lote de resultados de un plugin para una consulta. Puede venir vacío.
#[derive(Debug, Clone)]
pub struct QueryBatch {
    /// Consulta a la que pertenecen.
    pub id: QueryId,
    /// Plugin que los produjo.
    pub plugin_id: String,
    /// Resultados ya puntuados y ordenados.
    pub items: Vec<ResultItem>,
}

/// Receptor de lotes. Se llama desde los hilos del pool.
pub type Sink = Arc<dyn Fn(QueryBatch) + Send + Sync>;

struct Registered {
    plugin: Arc<dyn Plugin>,
    meta: PluginMetadata,
}

struct Job {
    plugin: Arc<dyn Plugin>,
    meta: PluginMetadata,
    query: Query,
    cancel: CancelToken,
    sink: Sink,
    /// La consulta es exactamente la palabra clave del plugin: sus resultados van arriba.
    boost: bool,
}

struct Shared {
    frecency: Mutex<Frecency>,
    frecency_dirty: AtomicBool,
    config: RwLock<EngineConfig>,
}

/// Motor de búsqueda.
pub struct Engine {
    plugins: RwLock<Arc<Vec<Registered>>>,
    shared: Arc<Shared>,
    jobs: Sender<Job>,
    next_id: AtomicU64,
    current: Mutex<CancelToken>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Engine {
    /// Crea el motor con `workers` hilos (mínimo 1).
    pub fn new(workers: usize, config: EngineConfig, frecency: Frecency) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded::<Job>();
        let shared = Arc::new(Shared {
            frecency: Mutex::new(frecency),
            frecency_dirty: AtomicBool::new(false),
            config: RwLock::new(config),
        });
        for n in 0..workers.max(1) {
            let rx = rx.clone();
            let shared = Arc::clone(&shared);
            let spawned = std::thread::Builder::new()
                .name(format!("rukes-search-{n}"))
                .spawn(move || worker(&rx, &shared));
            if let Err(e) = spawned {
                tracing::error!(error = %e, "no se pudo crear un hilo de búsqueda");
            }
        }
        Self {
            plugins: RwLock::new(Arc::new(Vec::new())),
            shared,
            jobs: tx,
            next_id: AtomicU64::new(0),
            current: Mutex::new(CancelToken::new()),
        }
    }

    /// Sustituye los plugins activos (en orden de prioridad).
    pub fn set_plugins(&self, plugins: Vec<Arc<dyn Plugin>>) {
        let regs = plugins
            .into_iter()
            .map(|plugin| Registered {
                meta: plugin.metadata(),
                plugin,
            })
            .collect();
        if let Ok(mut p) = self.plugins.write() {
            *p = Arc::new(regs);
        }
    }

    /// Vuelve a leer los metadatos de los plugins (prefijos y palabras clave que cambian en
    /// caliente, como los alias de scripts).
    pub fn reload_metadata(&self) {
        let current = self.snapshot();
        let regs = current
            .iter()
            .map(|r| Registered {
                meta: r.plugin.metadata(),
                plugin: Arc::clone(&r.plugin),
            })
            .collect();
        if let Ok(mut p) = self.plugins.write() {
            *p = Arc::new(regs);
        }
    }

    /// Actualiza los ajustes.
    pub fn set_config(&self, config: EngineConfig) {
        if let Ok(mut c) = self.shared.config.write() {
            *c = config;
        }
    }

    fn snapshot(&self) -> Arc<Vec<Registered>> {
        self.plugins
            .read()
            .map(|p| Arc::clone(&p))
            .unwrap_or_default()
    }

    /// Lanza una consulta. Cancela la anterior. Los resultados llegan a `sink` por lotes; una
    /// consulta que no se enruta a ningún plugin no produce lotes.
    pub fn query(&self, raw: &str, sink: Sink) -> QueryId {
        let id = QueryId(self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let cancel = CancelToken::new();
        {
            let mut cur = lock(&self.current);
            cur.cancel();
            *cur = cancel.clone();
        }
        let plugins = self.snapshot();
        for Route {
            idx,
            text,
            targeted,
            boost,
        } in route(&plugins, raw)
        {
            let Some(reg) = plugins.get(idx) else {
                continue;
            };
            let job = Job {
                plugin: Arc::clone(&reg.plugin),
                meta: reg.meta.clone(),
                query: Query {
                    id,
                    raw: raw.to_owned(),
                    text,
                    targeted,
                },
                cancel: cancel.clone(),
                sink: Arc::clone(&sink),
                boost,
            };
            if self.jobs.send(job).is_err() {
                tracing::error!("el pool de búsqueda no está disponible");
            }
        }
        id
    }

    /// Cancela la consulta en curso.
    pub fn cancel(&self) {
        lock(&self.current).cancel();
    }

    /// Ejecuta una acción sobre un resultado y registra el uso para la frecency.
    pub fn execute(
        &self,
        raw_query: &str,
        item: &ResultItem,
        action: &str,
    ) -> Result<ActionOutcome, PluginError> {
        let plugins = self.snapshot();
        let reg = plugins
            .iter()
            .find(|r| r.meta.id == item.plugin_id)
            .ok_or_else(|| {
                PluginError::Failed(format!("plugin desconocido: {}", item.plugin_id))
            })?;
        let result = catch_unwind(AssertUnwindSafe(|| reg.plugin.execute(item, action)))
            .unwrap_or_else(|_| {
                tracing::error!(plugin = %item.plugin_id, "pánico al ejecutar una acción");
                Err(PluginError::Failed("el plugin falló".into()))
            });
        let mut result = result;
        match &mut result {
            Ok(ActionOutcome::ShowResults(items)) => {
                // Un submenú o confirmación no cuenta como uso.
                for (n, it) in items.iter_mut().enumerate() {
                    it.plugin_id.clone_from(&item.plugin_id);
                    it.score = 1.0 - n as f32 * 0.001;
                }
            }
            Ok(_) => {
                lock(&self.shared.frecency).record(raw_query, &item.key(), now_secs());
                self.shared.frecency_dirty.store(true, Ordering::Relaxed);
            }
            Err(_) => {}
        }
        result
    }

    /// Lanza `refresh` de todos los plugins en un hilo aparte.
    pub fn refresh_all(&self) {
        let plugins = self.snapshot();
        let spawned = std::thread::Builder::new()
            .name("rukes-refresh".into())
            .spawn(move || {
                for r in plugins.iter() {
                    if catch_unwind(AssertUnwindSafe(|| r.plugin.refresh())).is_err() {
                        tracing::error!(plugin = %r.meta.id, "pánico en refresh");
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::error!(error = %e, "no se pudo lanzar el refresco");
        }
    }

    /// Guarda la frecency si cambió desde el último guardado.
    pub fn save_frecency(&self, path: &Path) -> std::io::Result<()> {
        if !self.shared.frecency_dirty.swap(false, Ordering::Relaxed) {
            return Ok(());
        }
        let snapshot = lock(&self.shared.frecency).clone();
        crate::persist::save_json(path, &snapshot)
    }
}

/// A qué plugin va una consulta.
#[derive(Debug, PartialEq, Eq)]
struct Route {
    idx: usize,
    text: String,
    targeted: bool,
    boost: bool,
}

impl Route {
    fn new(idx: usize, text: String, targeted: bool) -> Self {
        Self {
            idx,
            text,
            targeted,
            boost: false,
        }
    }
}

/// Decide qué plugins reciben la consulta y con qué texto.
fn route(plugins: &[Registered], raw: &str) -> Vec<Route> {
    let trimmed = raw.trim_start();
    // 1. Prefijo (el más largo gana: `f:` antes que `f`).
    let by_prefix = plugins
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let p = r.meta.prefix.as_deref()?;
            (!p.is_empty() && trimmed.starts_with(p)).then_some((i, p.len()))
        })
        .max_by_key(|(_, len)| *len);
    if let Some((i, len)) = by_prefix {
        return vec![Route::new(i, trimmed[len..].trim().to_owned(), true)];
    }
    // 2. Palabra clave al principio, seguida de un espacio (`g` a secas sigue siendo una búsqueda
    // global; `g ` ya va al buscador web). Puede tener varias palabras (`gh issue`, alias de
    // scripts) y gana la más larga: `gh issue x` va al script antes que a la búsqueda `gh`.
    let by_keyword = plugins
        .iter()
        .enumerate()
        .flat_map(|(i, r)| r.meta.keywords.iter().map(move |k| (i, k)))
        .filter_map(|(i, k)| strip_keyword(trimmed, k).map(|rest| (i, k.len(), rest)))
        .max_by_key(|(_, len, _)| *len);
    if let Some((i, _, rest)) = by_keyword {
        return vec![Route::new(i, rest.trim().to_owned(), true)];
    }
    // 3. Plugins globales.
    let text = raw.trim();
    if text.is_empty() {
        return Vec::new();
    }
    // 4. La consulta es justo una palabra clave (`clip`): además de la búsqueda global, su
    // plugin muestra su lista arriba. Así no hace falta el espacio para verla.
    let exact: Vec<usize> = plugins
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.meta.list_on_keyword
                && r.meta
                    .keywords
                    .iter()
                    .any(|k| !k.is_empty() && k.to_lowercase() == text.to_lowercase())
        })
        .map(|(i, _)| i)
        .collect();
    let mut out: Vec<Route> = plugins
        .iter()
        .enumerate()
        .filter(|(i, r)| r.meta.global && !exact.contains(i))
        .map(|(i, _)| Route::new(i, text.to_owned(), false))
        .collect();
    out.extend(exact.into_iter().map(|i| Route {
        boost: true,
        ..Route::new(i, String::new(), true)
    }));
    out
}

/// Si `text` empieza por la palabra clave `keyword` (sin distinguir mayúsculas) seguida de un
/// espacio, devuelve el resto.
fn strip_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    if keyword.is_empty() {
        return None;
    }
    let mut chars = text.char_indices();
    for k in keyword.chars() {
        let (_, c) = chars.next()?;
        if !c.to_lowercase().eq(k.to_lowercase()) {
            return None;
        }
    }
    let (idx, next) = chars.next()?;
    next.is_whitespace().then(|| &text[idx..])
}

fn worker(rx: &Receiver<Job>, shared: &Shared) {
    while let Ok(job) = rx.recv() {
        if job.cancel.is_cancelled() {
            continue;
        }
        let (weight, limit) = shared
            .config
            .read()
            .map(|c| (c.frecency_weight, c.max_results_per_plugin))
            .unwrap_or((0.35, 50));
        let Job {
            plugin,
            meta,
            query,
            cancel,
            sink,
            boost,
        } = job;
        let mut deliver = |mut items: Vec<ResultItem>| {
            // Los lotes vacíos se entregan igualmente: indican a la UI que el plugin terminó
            // sin resultados y que puede retirar los de la consulta anterior.
            if cancel.is_cancelled() {
                return;
            }
            let now = now_secs();
            {
                let f = lock(&shared.frecency);
                for it in &mut items {
                    it.plugin_id.clone_from(&meta.id);
                    let key = it.key();
                    it.score = ranking::final_score(
                        ScoreInput {
                            fuzzy: it.fuzzy,
                            item_frecency: f.item_score(&key, now),
                            query_frecency: f.query_score(&query.raw, &key, now),
                            alias_match: it.alias_match || boost,
                            priority: meta.priority,
                        },
                        weight,
                    );
                }
            }
            items.sort_by(ranking::compare);
            items.truncate(limit);
            sink(QueryBatch {
                id: query.id,
                plugin_id: meta.id.clone(),
                items,
            });
        };
        let res = catch_unwind(AssertUnwindSafe(|| {
            plugin.query(&query, &cancel, &mut deliver);
        }));
        if res.is_err() {
            tracing::error!(plugin = %meta.id, "pánico en query; el launcher sigue funcionando");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fuzzy;
    use std::time::Duration;

    struct ListPlugin {
        id: &'static str,
        prefix: Option<&'static str>,
        keywords: Vec<String>,
        global: bool,
        items: Vec<&'static str>,
        delay: Duration,
        panic: bool,
    }

    impl ListPlugin {
        fn new(id: &'static str, items: Vec<&'static str>) -> Self {
            Self {
                id,
                prefix: None,
                keywords: vec![],
                global: true,
                items,
                delay: Duration::ZERO,
                panic: false,
            }
        }
    }

    impl Plugin for ListPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata {
                id: self.id.into(),
                name: self.id.into(),
                prefix: self.prefix.map(Into::into),
                keywords: self.keywords.clone(),
                priority: 0.0,
                global: self.global,
                list_on_keyword: !self.global,
            }
        }
        fn query(&self, q: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
            assert!(!self.panic, "boom");
            std::thread::sleep(self.delay);
            if cancel.is_cancelled() {
                return;
            }
            let mut f = Fuzzy::new(&q.text);
            let out = self
                .items
                .iter()
                .filter_map(|t| {
                    let s = f.score(t)?;
                    let mut r = ResultItem::new(*t, *t);
                    r.fuzzy = s;
                    Some(r)
                })
                .collect();
            sink(out);
        }
        fn execute(&self, _: &ResultItem, _: &str) -> Result<ActionOutcome, PluginError> {
            Ok(ActionOutcome::Hide)
        }
    }

    fn collect(engine: &Engine, q: &str, wait: Duration) -> Vec<QueryBatch> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let tx = Mutex::new(tx);
        engine.query(
            q,
            Arc::new(move |b| {
                let _ = lock(&tx).send(b);
            }),
        );
        let mut out = vec![];
        while let Ok(b) = rx.recv_timeout(wait) {
            out.push(b);
        }
        out
    }

    fn engine(plugins: Vec<ListPlugin>) -> Engine {
        let e = Engine::new(2, EngineConfig::default(), Frecency::default());
        e.set_plugins(plugins.into_iter().map(|p| Arc::new(p) as _).collect());
        e
    }

    #[test]
    fn global_query_reaches_global_plugins() {
        let e = engine(vec![
            ListPlugin::new("apps", vec!["Google Chrome", "Code"]),
            ListPlugin {
                global: false,
                ..ListPlugin::new("hidden", vec!["Chrome"])
            },
        ]);
        let b = collect(&e, "chr", Duration::from_millis(200));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].plugin_id, "apps");
        assert_eq!(b[0].items[0].title, "Google Chrome");
    }

    #[test]
    fn prefix_and_keyword_target_one_plugin() {
        let e = engine(vec![
            ListPlugin::new("apps", vec!["kill"]),
            ListPlugin {
                prefix: Some("="),
                global: false,
                ..ListPlugin::new("calc", vec!["5 km"])
            },
            ListPlugin {
                keywords: vec!["kill".into()],
                global: false,
                ..ListPlugin::new("proc", vec!["chrome.exe"])
            },
        ]);
        let b = collect(&e, "= 5", Duration::from_millis(200));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].plugin_id, "calc");
        let b = collect(&e, "kill chr", Duration::from_millis(200));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].plugin_id, "proc");
        // La palabra clave de varias palabras más larga gana.
        let e2 = engine(vec![
            ListPlugin {
                keywords: vec!["gh".into()],
                global: false,
                ..ListPlugin::new("web", vec!["issue x"])
            },
            ListPlugin {
                keywords: vec!["GH Issue".into(), "día".into()],
                global: false,
                ..ListPlugin::new("scripts", vec!["mi bug"])
            },
        ]);
        let b = collect(&e2, "gh issue mi bug", Duration::from_millis(200));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].plugin_id, "scripts");
        let b = collect(&e2, "gh issues x", Duration::from_millis(200));
        assert_eq!(b[0].plugin_id, "web");
        let b = collect(&e2, "DÍA mi", Duration::from_millis(200));
        assert_eq!(b[0].plugin_id, "scripts");
        // Sin espacio, la palabra clave no dirige la búsqueda: se busca globalmente y además su
        // plugin muestra su lista, arriba.
        let mut b = collect(&e, "kill", Duration::from_millis(200));
        assert_eq!(b.len(), 2);
        b.sort_by(|x, y| x.plugin_id.cmp(&y.plugin_id));
        assert_eq!(b[0].plugin_id, "apps");
        assert_eq!(b[1].plugin_id, "proc");
        assert!(b[1].items[0].score > b[0].items[0].score);
    }

    #[test]
    fn empty_query_produces_nothing() {
        let e = engine(vec![ListPlugin::new("apps", vec!["a"])]);
        assert!(collect(&e, "   ", Duration::from_millis(100)).is_empty());
    }

    #[test]
    fn slow_plugin_does_not_block_fast_one_and_is_cancelled() {
        let e = engine(vec![
            ListPlugin {
                delay: Duration::from_millis(300),
                ..ListPlugin::new("slow", vec!["chrome slow"])
            },
            ListPlugin::new("fast", vec!["chrome fast"]),
        ]);
        let (tx, rx) = crossbeam_channel::unbounded();
        let tx = Mutex::new(tx);
        e.query(
            "chrome",
            Arc::new(move |b: QueryBatch| {
                let _ = lock(&tx).send(b.plugin_id);
            }),
        );
        // El rápido llega antes de que termine el lento.
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(150)).ok().as_deref(),
            Some("fast")
        );
        // Una consulta nueva cancela la anterior: el lento ya no entrega.
        e.query("zzz", Arc::new(|_| {}));
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    }

    #[test]
    fn panicking_plugin_is_isolated() {
        let e = engine(vec![
            ListPlugin {
                panic: true,
                ..ListPlugin::new("bad", vec![])
            },
            ListPlugin::new("good", vec!["chrome"]),
        ]);
        let b = collect(&e, "chrome", Duration::from_millis(200));
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].plugin_id, "good");
        // El pool sigue vivo.
        assert_eq!(collect(&e, "chrome", Duration::from_millis(200)).len(), 1);
    }

    #[test]
    fn execute_records_frecency_and_reorders() {
        let e = engine(vec![ListPlugin::new(
            "apps",
            vec!["Chrome Canary", "Chrome"],
        )]);
        let first = collect(&e, "chrome", Duration::from_millis(200));
        let canary = first[0]
            .items
            .iter()
            .find(|i| i.title == "Chrome Canary")
            .cloned()
            .unwrap();
        assert_eq!(first[0].items[0].title, "Chrome");
        for _ in 0..5 {
            e.execute("chrome", &canary, "open").unwrap();
        }
        let second = collect(&e, "chrome", Duration::from_millis(200));
        assert_eq!(second[0].items[0].title, "Chrome Canary");
    }
}
