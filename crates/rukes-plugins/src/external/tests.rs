//! Tests con un plugin falso en PowerShell (siempre presente en Windows).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rukes_core::{
    ActionOutcome, CancelToken, Engine, EngineConfig, Frecency, Plugin, PluginError,
    PluginMetadata, Query, QueryId, ResultItem,
};

use super::*;

const FAKE: &str = r#"
$ErrorActionPreference = 'Stop'
function Send($o) { [Console]::Out.WriteLine(($o | ConvertTo-Json -Compress -Depth 8)); [Console]::Out.Flush() }
while ($true) {
  $line = [Console]::In.ReadLine()
  if ($null -eq $line) { break }
  $m = $line | ConvertFrom-Json
  switch ($m.method) {
    'initialize' { [Console]::Error.WriteLine('init'); Send @{ id = $m.id; result = @{ name = 'fake' } } }
    'query' {
      $t = $m.params.text
      if ($t -eq 'cuelga') { Start-Sleep -Seconds 30 }
      elseif ($t -eq 'muere') { exit 3 }
      elseif ($t -eq 'error') { Send @{ id = $m.id; error = @{ message = 'malo' } } }
      elseif ($t -eq 'stream') {
        Send @{ method = 'results'; params = @{ query_id = $m.params.query_id; items = @(@{ id = 'a'; title = 'primero' }) } }
        Send @{ id = $m.id; result = @{ items = @(@{ id = 'b'; title = 'segundo' }) } }
      }
      else { Send @{ id = $m.id; result = @(@{ id = 'x'; title = "eco $t"; data = @{ n = 1 }; actions = @('open', @{ id = 'copy'; label = 'Copiar' }) }) } }
    }
    'action' { Send @{ id = $m.id; result = @{ notify = "accion $($m.params.action) $($m.params.item.id) $($m.params.item.data.n)" } } }
  }
}
"#;

fn fake(tag: &str, global: bool) -> (PathBuf, Arc<ExternalPlugin>) {
    let _ = rukes_core::i18n::set_language("es", "");
    let root = std::env::temp_dir().join(format!("rukes-ext-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("fake");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fake.ps1"), FAKE).unwrap();
    std::fs::write(
        dir.join(MANIFEST),
        format!(
            "[plugin]\nname = \"Fake\"\nversion = \"0.1\"\nexecutable = \"powershell.exe\"\n\
             args = [\"-NoProfile\", \"-ExecutionPolicy\", \"Bypass\", \"-File\", \"fake.ps1\"]\n\
             keywords = [\"fake\"]\nglobal = {global}\n\n[permissions]\nnetwork = true\n"
        ),
    )
    .unwrap();
    let set = ExternalPlugins::load(&root);
    assert!(set.errors.is_empty(), "{:?}", set.errors);
    let p = Arc::clone(&set.plugins[0]);
    (root, p)
}

fn ask(p: &ExternalPlugin, text: &str) -> Vec<Vec<ResultItem>> {
    ask_with(p, text, &CancelToken::new())
}

fn ask_with(p: &ExternalPlugin, text: &str, cancel: &CancelToken) -> Vec<Vec<ResultItem>> {
    let q = Query {
        id: QueryId(7),
        raw: format!("fake {text}"),
        text: text.into(),
        targeted: true,
    };
    let mut out = Vec::new();
    p.query(&q, cancel, &mut |b| out.push(b));
    out
}

#[test]
fn answers_streams_and_runs_actions() {
    let (root, p) = fake("ok", false);
    let r = ask(&p, "hola");
    assert_eq!(r.len(), 1);
    assert_eq!(r[0][0].title, "eco hola");
    assert_eq!(r[0][0].actions.len(), 2);
    assert!(p.process.is_running());
    match p.execute(&r[0][0], "copy").unwrap() {
        ActionOutcome::Notify(s) => assert_eq!(s, "accion copy x 1"),
        other => panic!("{other:?}"),
    }
    let r = ask(&p, "stream");
    let titles: Vec<&str> = r.iter().flatten().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["primero", "segundo"]);
    // Un error del plugin se muestra, pero no cuenta como fallo.
    let r = ask(&p, "error");
    assert_eq!(r[0][0].id, "err");
    assert_eq!(p.process.failures(), 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn hang_times_out_and_recovers() {
    let (root, p) = fake("hang", false);
    assert_eq!(ask(&p, "hola")[0][0].title, "eco hola");
    let t = Instant::now();
    let r = ask(&p, "cuelga");
    let took = t.elapsed();
    assert!(
        took >= QUERY_TIMEOUT && took < QUERY_TIMEOUT + Duration::from_secs(1),
        "{took:?}"
    );
    assert_eq!(r[0][0].id, "err");
    assert_eq!(p.process.failures(), 1);
    assert!(!p.process.is_running(), "el proceso colgado se para");
    // Se vuelve a arrancar solo y el contador vuelve a cero.
    assert_eq!(ask(&p, "hola")[0][0].title, "eco hola");
    assert_eq!(p.process.failures(), 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cancel_releases_at_once_without_counting() {
    let (root, p) = fake("cancel", false);
    assert_eq!(ask(&p, "hola").len(), 1);
    let cancel = CancelToken::new();
    let c = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        c.cancel();
    });
    let t = Instant::now();
    let r = ask_with(&p, "cuelga", &cancel);
    assert!(
        t.elapsed() < Duration::from_millis(600),
        "{:?}",
        t.elapsed()
    );
    assert!(r.is_empty());
    assert_eq!(p.process.failures(), 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn three_deaths_disable_until_restart() {
    let (root, p) = fake("die", false);
    for n in 1..=MAX_FAILURES {
        let r = ask(&p, "muere");
        assert_eq!(r[0][0].id, "err");
        assert_eq!(p.process.failures(), n);
    }
    assert!(p.process.is_disabled());
    let r = ask(&p, "hola");
    assert_eq!(r[0][0].id, "err");
    assert!(r[0][0].title.contains("desactivado"), "{}", r[0][0].title);
    // Enter sobre el error lo reinicia.
    let item = r[0][0].clone();
    assert!(matches!(
        p.execute(&item, "open"),
        Ok(ActionOutcome::Notify(_))
    ));
    assert_eq!(ask(&p, "hola")[0][0].title, "eco hola");
    let _ = std::fs::remove_dir_all(root);
}

struct Fast;

impl Plugin for Fast {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "fast".into(),
            name: "fast".into(),
            prefix: None,
            keywords: vec![],
            priority: 0.0,
            global: true,
            list_on_keyword: false,
        }
    }
    fn query(&self, q: &Query, _: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let mut it = ResultItem::new("1", q.text.clone());
        it.fuzzy = 1.0;
        sink(vec![it]);
    }
    fn execute(&self, _: &ResultItem, _: &str) -> Result<ActionOutcome, PluginError> {
        Ok(ActionOutcome::Hide)
    }
}

#[test]
fn hanging_plugin_does_not_delay_the_rest() {
    let (root, p) = fake("engine", true);
    let engine = Engine::new(2, EngineConfig::default(), Frecency::default());
    engine.set_plugins(vec![p as Arc<dyn Plugin>, Arc::new(Fast)]);
    let (tx, rx) = crossbeam_channel::unbounded();
    let tx = Mutex::new(tx);
    let t = Instant::now();
    engine.query(
        "cuelga",
        Arc::new(move |b| {
            let _ = tx
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .send((b.plugin_id, t.elapsed()));
        }),
    );
    let (id, took) = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(id, "fast");
    assert!(took < Duration::from_millis(200), "{took:?}");
    // Sin consulta nueva, el externo acaba entregando (vacío) tras su tiempo máximo.
    let (id, took) = rx.recv_timeout(Duration::from_secs(9)).unwrap();
    assert!(id.starts_with("ext:"));
    assert!(took >= QUERY_TIMEOUT, "{took:?}");
    engine.cancel();
    let _ = std::fs::remove_dir_all(root);
}

/// El ejemplo en Python del repositorio funciona con el host (si hay `python`).
#[test]
fn python_example_plugin_works() {
    let ok = std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("sin python: se omite");
        return;
    }
    let _ = rukes_core::i18n::set_language("es", "");
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins-examples/cuenta");
    let root = std::env::temp_dir().join(format!("rukes-ext-py-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("cuenta");
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["plugin.toml", "main.py"] {
        std::fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    let set = ExternalPlugins::load(&root);
    assert!(set.errors.is_empty(), "{:?}", set.errors);
    let p = &set.plugins[0];
    assert_eq!(p.manifest.permissions_label(), "clipboard");
    let r = ask(p, "uno dos tres");
    assert_eq!(r[0][0].title, "3 palabras");
    match p.execute(&r[0][0], "notify").unwrap() {
        ActionOutcome::Notify(s) => assert_eq!(s, "palabras: 3"),
        other => panic!("{other:?}"),
    }
    assert_eq!(ask(p, "muere")[0][0].id, "err");
    assert_eq!(ask(p, "hola")[0][0].title, "1 palabra");
    drop(set);
    let _ = std::fs::remove_dir_all(root);
}
