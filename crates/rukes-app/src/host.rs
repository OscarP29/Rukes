//! Orquestación: implementa [`rukes_ui::Host`] y conecta el motor, la plataforma y la config.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use rukes_config::DirWatcher;
use rukes_config::theme::Theme;
use rukes_config::{Config, ConfigError, Hotkey};
use rukes_core::i18n::{tr, trf};
use rukes_core::{ActionOutcome, Engine, Icon, Plugin, QueryId, ResultItem};
use rukes_platform::clipwatch::{Change, ClipboardWatcher};
use rukes_platform::hotkey::{self, HotkeyManager, HotkeySet};
use rukes_platform::tray::{MenuEntry, Tray};
use rukes_platform::{autostart, dwm, icons, window};
use rukes_plugins::workflows::{Hooks as WorkflowHooks, Runner, Scheduler, Target};
use rukes_plugins::{
    AppsPlugin, CalcPlugin, ClipHistory, ClipboardPlugin, ExternalPlugins, FileIndex, FilesPlugin,
    GeneratorsPlugin, NotesPlugin, ProcessesPlugin, RemindPlugin, RemindStore, ShellPlugin,
    ShellSettings, SnippetStore, SnippetsPlugin, SystemCommandsPlugin, TodoPlugin, WallPlugin,
    WebPlugin, WindowsPlugin, WorkflowStore, WorkflowsPlugin,
};
use rukes_script::{Catalog, Hooks, RunContext, Runs, Scripts, ScriptsPlugin};
use rukes_ui::{Host, IconRgba, Preview, Sink, UiConfig, UiEvent, UiSender, UiStyle};

use crate::iconcache::IconCache;
use crate::look::{self, System};
use crate::settings;
use crate::system_plugin::SystemPlugin;

/// Antigüedad del índice de apps a partir de la cual se reindexa al mostrar la ventana.
const REINDEX_AFTER: Duration = Duration::from_secs(10 * 60);
/// Espera tras ocultar antes de liberar el working set (por si se vuelve a mostrar enseguida).
const TRIM_AFTER: Duration = Duration::from_millis(1500);

const TRAY_SHOW: u16 = 1;
const TRAY_SETTINGS: u16 = 2;
const TRAY_RELOAD: u16 = 3;
const TRAY_PAUSE: u16 = 4;
const TRAY_QUIT: u16 = 5;
const TRAY_TRIGGERS: u16 = 6;

/// Nombre del valor en la clave `Run`.
const AUTOSTART_NAME: &str = "Rukes";

/// Activa o quita el inicio con Windows según la config (RF-008).
pub fn apply_autostart(config: &Config) {
    let command = config
        .general
        .start_with_windows
        .then(|| std::env::current_exe().ok())
        .flatten()
        .map(|exe| format!("\"{}\"", exe.display()));
    match autostart::set(AUTOSTART_NAME, command.as_deref()) {
        Ok(()) => tracing::debug!(enabled = command.is_some(), "inicio con Windows aplicado"),
        Err(e) => tracing::warn!(%e, "no se pudo cambiar el inicio con Windows"),
    }
}

struct Inner {
    config: Config,
    hotkey: Option<HotkeyManager>,
    hotkey_text: String,
    last_refresh: Instant,
    /// Último tema válido.
    theme: Theme,
    /// Estado del sistema con que se calculó el aspecto.
    system: System,
    /// Aspecto enviado a la UI.
    style: UiStyle,
    /// Fondo pedido a DWM (`backdrop`, oscuro) y el que se aplicó de verdad.
    dwm: Option<((dwm::Backdrop, bool), dwm::Backdrop)>,
}

pub struct AppHost {
    engine: Arc<Engine>,
    ui: UiSender,
    paths: rukes_config::paths::Paths,
    hwnd: AtomicIsize,
    /// Ventana que tenía el foco antes de mostrar el launcher.
    previous: AtomicIsize,
    /// Contador de ocultaciones, para cancelar recortes de memoria obsoletos.
    hide_seq: std::sync::atomic::AtomicU64,
    shown_at: Mutex<Option<Instant>>,
    inner: Mutex<Inner>,
    me: Mutex<Weak<AppHost>>,
    /// Índice de archivos; se conserva entre recargas de config si sus ajustes no cambian.
    files: Mutex<Option<Arc<FileIndex>>>,
    tray: Mutex<Option<Tray>>,
    paused: AtomicBool,
    /// Catálogo de scripts con la carpeta e intérpretes con que se abrió.
    scripts: Mutex<Option<ScriptCatalog>>,
    /// Historial de ejecuciones de scripts.
    runs: Mutex<Option<Arc<Runs>>>,
    icon_cache: IconCache,
    /// Historial del portapapeles y su escucha (se sueltan al desactivar el plugin, RF-104).
    clip: Mutex<Option<(Arc<ClipHistory>, ClipboardWatcher)>>,
    /// Snippets con su carpeta y su vigilante.
    snippets: Mutex<Option<SnippetSlot>>,
    /// Recordatorios (un solo hilo de aviso aunque se recarguen los plugins).
    reminders: Mutex<Option<Arc<RemindStore>>>,
    /// Plugin de scripts vigente (para ejecutar scripts desde workflows, triggers y atajos).
    scripts_plugin: Mutex<Option<Arc<ScriptsPlugin>>>,
    /// Workflows: carpeta, ejecutor, planificador y vigilante.
    workflows: Mutex<Option<WorkflowSlot>>,
    /// Atajos propios de scripts y workflows (RF-162) y lo que ejecuta cada uno.
    extra_hotkeys: Mutex<Option<HotkeySet>>,
    hotkey_targets: Arc<Mutex<Vec<Target>>>,
    /// Plugins externos: carpeta, plugins cargados y vigilante (RF-170, RF-173).
    external: Mutex<Option<ExternalSlot>>,
    /// Fondos de pantalla (se conserva si no cambian sus ajustes, para no reiniciar la rotación).
    wall: Mutex<Option<Arc<WallPlugin>>>,
}

type ExternalSlot = (std::path::PathBuf, Arc<ExternalPlugins>, Option<DirWatcher>);

type WorkflowSlot = (
    std::path::PathBuf,
    Arc<Runner>,
    Arc<Scheduler>,
    Option<DirWatcher>,
);

type SnippetSlot = (std::path::PathBuf, Arc<SnippetStore>, Option<DirWatcher>);

/// Espera a que la ventana anterior recupere el foco antes de pegar.
const PASTE_DELAY: Duration = Duration::from_millis(80);
/// Espera tras pegar antes de devolver el portapapeles anterior (la app destino lo lee antes).
const RESTORE_DELAY: Duration = Duration::from_millis(400);

type ScriptCatalog = (
    std::path::PathBuf,
    std::collections::BTreeMap<String, String>,
    Arc<Catalog>,
);

impl AppHost {
    pub fn new(
        engine: Arc<Engine>,
        ui: UiSender,
        paths: rukes_config::paths::Paths,
        config: &Config,
    ) -> Self {
        let icon_cache = IconCache::open(paths.cache_dir.join("icons"));
        Self {
            engine,
            ui,
            icon_cache,
            paths,
            hwnd: AtomicIsize::new(0),
            previous: AtomicIsize::new(0),
            hide_seq: std::sync::atomic::AtomicU64::new(0),
            shown_at: Mutex::new(None),
            inner: Mutex::new(Inner {
                config: config.clone(),
                hotkey: None,
                hotkey_text: String::new(),
                last_refresh: Instant::now(),
                theme: Theme::default(),
                system: System::default(),
                style: UiStyle::default(),
                dwm: None,
            }),
            me: Mutex::new(Weak::new()),
            files: Mutex::new(None),
            tray: Mutex::new(None),
            paused: AtomicBool::new(false),
            scripts: Mutex::new(None),
            runs: Mutex::new(None),
            clip: Mutex::new(None),
            snippets: Mutex::new(None),
            reminders: Mutex::new(None),
            scripts_plugin: Mutex::new(None),
            workflows: Mutex::new(None),
            extra_hotkeys: Mutex::new(None),
            hotkey_targets: Arc::default(),
            external: Mutex::new(None),
            wall: Mutex::new(None),
        }
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn notice(&self, text: String) {
        let _ = self.ui.unbounded_send(UiEvent::Notice(text));
    }

    /// Ajustes de la UI con el aspecto actual.
    pub fn ui_config(&self) -> UiConfig {
        let inner = self.inner();
        settings::ui(
            &inner.config,
            inner.style.clone(),
            inner.system.animations,
            inner.system.text_scale,
        )
    }

    /// Aplica a la ventana el fondo y el modo oscuro que tocan, si han cambiado.
    fn apply_dwm(&self, inner: &mut Inner) {
        let hwnd = self.hwnd.load(Ordering::SeqCst);
        if hwnd == 0 {
            return;
        }
        let want = (
            look::backdrop(&inner.config, &inner.theme, &inner.system),
            look::dark(&inner.theme, &inner.system),
        );
        if inner.dwm.is_some_and(|(w, _)| w == want) {
            return;
        }
        let applied = dwm::apply(hwnd, want.0, want.1);
        tracing::info!(?want, ?applied, "fondo de la ventana");
        inner.dwm = Some((want, applied));
    }

    /// Recalcula el aspecto (tema, modo del sistema, contraste alto, escalado, animaciones) y
    /// lo envía a la UI si ha cambiado. `reload` vuelve a leer el archivo del tema (RF-193).
    pub fn refresh_look(&self, reload: bool) {
        let sys = System::read();
        let mut inner = self.inner();
        let name_changed =
            look::theme_name(&inner.config, &sys) != look::theme_name(&inner.config, &inner.system);
        if !reload && !name_changed && sys == inner.system {
            return;
        }
        if reload || name_changed {
            match look::load_theme(&inner.config, &sys, &self.paths.themes_dir()) {
                Ok(t) => {
                    tracing::info!(theme = %t.name, "tema cargado");
                    inner.theme = t;
                }
                Err(e) => {
                    tracing::warn!(%e, "tema no válido; se mantiene el anterior");
                    self.notice(format!("{}: {e}", tr("error.theme")));
                }
            }
        }
        inner.system = sys;
        self.apply_dwm(&mut inner);
        let translucent = inner
            .dwm
            .is_some_and(|(_, applied)| applied != dwm::Backdrop::Solid);
        inner.style = look::ui_style(&inner.theme, &inner.system, translucent);
        let ui = settings::ui(
            &inner.config,
            inner.style.clone(),
            inner.system.animations,
            inner.system.text_scale,
        );
        drop(inner);
        let _ = self.ui.unbounded_send(UiEvent::Config(ui));
    }

    /// Vuelve a cargar los textos (al guardar un archivo de la carpeta `lang`, RF-211).
    pub fn reload_lang(&self) {
        let language = self.inner().config.general.language.clone();
        match rukes_core::i18n::set_language(&language, &rukes_platform::sysinfo::user_locale()) {
            Ok(()) => tracing::info!("textos recargados"),
            Err(e) => self.notice(format!("{}: {e}", tr("error.lang"))),
        }
        // Fuerza un nuevo dibujado con los textos nuevos.
        let ui = self.ui_config();
        let _ = self.ui.unbounded_send(UiEvent::Config(ui));
    }

    /// Crea los plugins según la config (RF-205: activar/desactivar y prioridad).
    pub fn install_plugins(self: &Arc<Self>, config: &Config) {
        *self.me.lock().unwrap_or_else(|p| p.into_inner()) = Arc::downgrade(self);
        let mut plugins: Vec<Arc<dyn Plugin>> = Vec::new();
        if config.plugins.apps.enabled {
            plugins.push(Arc::new(AppsPlugin::new(
                settings::apps(config),
                rukes_plugins::apps::cache_file(&self.paths.cache_dir),
            )));
        }
        if config.plugins.calc.enabled {
            plugins.push(Arc::new(CalcPlugin {
                prefix: config.plugins.calc.prefix.clone(),
                auto: config.plugins.calc.auto,
            }));
        }
        if config.plugins.web.enabled {
            plugins.push(Arc::new(WebPlugin {
                engines: config.plugins.web.engines.clone(),
            }));
        }
        let sh = &config.plugins.shell;
        if sh.enabled {
            plugins.push(Arc::new(ShellPlugin {
                settings: ShellSettings {
                    prefix: sh.prefix.clone(),
                    shell: sh.shell.clone(),
                    terminal: config.general.terminal.clone(),
                    keep_open: sh.keep_open,
                },
                cwd: std::env::var_os("USERPROFILE").map(std::path::PathBuf::from),
            }));
            plugins.push(Arc::new(SystemCommandsPlugin {
                confirm: sh.confirm_system,
            }));
        }
        let g = &config.plugins.generators;
        if g.enabled {
            plugins.push(Arc::new(GeneratorsPlugin {
                password_length: g.password_length,
                password_symbols: g.password_symbols,
            }));
        }
        if config.plugins.windows.enabled {
            plugins.push(Arc::new(WindowsPlugin {
                priority: config.plugins.windows.priority,
            }));
        }
        if config.plugins.processes.enabled {
            plugins.push(Arc::new(ProcessesPlugin::new(
                config.plugins.processes.priority,
            )));
        }
        let mut files = self.files.lock().unwrap_or_else(|p| p.into_inner());
        if config.plugins.files.enabled {
            let wanted = settings::files(config);
            let index = match files.as_ref() {
                Some(i) if *i.settings() == wanted => Arc::clone(i),
                _ => {
                    let i = FileIndex::new(wanted, self.paths.cache_dir.join("files.bin"));
                    i.rescan(false);
                    i
                }
            };
            for v in FilesPlugin::variants(&index) {
                plugins.push(Arc::new(v));
            }
            *files = Some(index);
        } else {
            *files = None;
        }
        drop(files);
        self.text_plugins(config, &mut plugins);
        self.workflow_plugins(config, &mut plugins);
        self.wall_plugin(config, &mut plugins);
        self.external_plugins(config, &mut plugins);
        if config.scripts.enabled {
            plugins.push(Arc::new(self.scripts_plugin(config)));
        } else {
            *self.scripts.lock().unwrap_or_else(|p| p.into_inner()) = None;
            *self
                .scripts_plugin
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = None;
        }
        let quit_host = Arc::downgrade(self);
        let reload_engine = Arc::clone(&self.engine);
        plugins.push(Arc::new(SystemPlugin {
            config_file: self.paths.config_file(),
            on_quit: Box::new(move || {
                if let Some(h) = quit_host.upgrade() {
                    h.quit();
                }
            }),
            on_reload: Box::new(move || reload_engine.refresh_all()),
        }));
        self.engine.set_plugins(plugins);
        self.refresh_hotkeys();
    }

    /// Workflows y triggers (secciones 5.14 y 5.15). El almacén, el planificador y el vigilante
    /// se conservan entre recargas si la carpeta no cambia.
    fn workflow_plugins(self: &Arc<Self>, config: &Config, plugins: &mut Vec<Arc<dyn Plugin>>) {
        let mut slot = self.workflows.lock().unwrap_or_else(|p| p.into_inner());
        if !config.workflows.enabled {
            *slot = None;
            return;
        }
        let dir = settings::workflows_dir(config, &self.paths);
        let (runner, scheduler) = match slot.as_ref() {
            Some((d, r, s, _)) if *d == dir => (Arc::clone(r), Arc::clone(s)),
            _ => {
                let (store, errors) = WorkflowStore::open(dir.clone());
                self.workflow_errors(&errors);
                let runner = Runner::new(Arc::clone(&store), self.workflow_hooks());
                let me = Arc::downgrade(self);
                let scheduler = Arc::new(Scheduler::spawn(
                    &store,
                    Arc::new(move |t: &rukes_plugins::workflows::Trigger| {
                        if let Some(h) = me.upgrade() {
                            h.run_target(&t.target);
                        }
                    }),
                    !config.workflows.triggers,
                ));
                let (weak, me, sched) = (
                    Arc::downgrade(&store),
                    Arc::downgrade(self),
                    Arc::downgrade(&scheduler),
                );
                let watcher = DirWatcher::spawn(&dir, move || {
                    if let (Some(s), Some(h)) = (weak.upgrade(), me.upgrade()) {
                        h.workflow_errors(&s.reload());
                        if let Some(sc) = sched.upgrade() {
                            sc.reload();
                        }
                        h.refresh_hotkeys();
                    }
                })
                .map_err(|e| tracing::warn!(%e, "no se pudo vigilar la carpeta de workflows"))
                .ok();
                *slot = Some((dir, Arc::clone(&runner), Arc::clone(&scheduler), watcher));
                (runner, scheduler)
            }
        };
        scheduler.set_paused(!config.workflows.triggers);
        plugins.push(Arc::new(WorkflowsPlugin {
            runner,
            scheduler: Some(scheduler),
        }));
    }

    /// Fondos de pantalla (RF-180).
    fn wall_plugin(&self, config: &Config, plugins: &mut Vec<Arc<dyn Plugin>>) {
        let mut slot = self.wall.lock().unwrap_or_else(|p| p.into_inner());
        if !config.plugins.wall.enabled {
            *slot = None;
            return;
        }
        let wanted = settings::wall(config);
        let wall = match slot.as_ref() {
            Some(w) if *w.settings() == wanted => Arc::clone(w),
            _ => Arc::new(WallPlugin::new(wanted)),
        };
        *slot = Some(Arc::clone(&wall));
        plugins.push(wall);
    }

    /// Plugins externos (RF-170..RF-173). Se conservan (con sus procesos) entre recargas de la
    /// configuración; la carpeta se vigila para instalar y quitar en caliente.
    fn external_plugins(self: &Arc<Self>, config: &Config, plugins: &mut Vec<Arc<dyn Plugin>>) {
        let mut slot = self.external.lock().unwrap_or_else(|p| p.into_inner());
        if !config.plugins.external.enabled {
            *slot = None;
            return;
        }
        let dir = settings::plugins_dir(config, &self.paths);
        let set = match slot.as_ref() {
            Some((d, set, _)) if *d == dir => Arc::clone(set),
            _ => {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    tracing::warn!(%e, dir = %dir.display(), "no se pudo crear la carpeta de plugins");
                }
                let set = Arc::new(ExternalPlugins::load(&dir));
                self.external_errors(&set.errors);
                tracing::info!(n = set.plugins.len(), dir = %dir.display(), "plugins externos");
                let me = Arc::downgrade(self);
                let watcher =
                    DirWatcher::spawn_tree(&dir, rukes_plugins::external::MANIFEST, move || {
                        if let Some(h) = me.upgrade() {
                            h.reload_external();
                        }
                    })
                    .map_err(|e| tracing::warn!(%e, "no se pudo vigilar la carpeta de plugins"))
                    .ok();
                *slot = Some((dir, Arc::clone(&set), watcher));
                set
            }
        };
        plugins.extend(set.engine_plugins());
    }

    /// Vuelve a leer la carpeta de plugins: avisa de los instalados (con sus permisos) y de los
    /// quitados, y recarga el motor. Los plugins desactivados por fallos vuelven a probarse.
    fn reload_external(self: &Arc<Self>) {
        let config = self.inner().config.clone();
        {
            let mut slot = self.external.lock().unwrap_or_else(|p| p.into_inner());
            let Some((dir, old, _)) = slot.as_mut() else {
                return;
            };
            let new = Arc::new(ExternalPlugins::load(dir));
            let ids = |s: &ExternalPlugins| -> std::collections::BTreeSet<String> {
                s.plugins.iter().map(|p| p.manifest.id.clone()).collect()
            };
            let (before, after) = (ids(old), ids(&new));
            for p in new
                .plugins
                .iter()
                .filter(|p| !before.contains(&p.manifest.id))
            {
                let m = &p.manifest;
                let perms = m.permissions_label();
                let perms = if perms.is_empty() {
                    tr("external.no_permissions").to_owned()
                } else {
                    perms
                };
                tracing::info!(plugin = %m.name, version = %m.version, permissions = %perms, "plugin externo instalado");
                self.system_notify(
                    &trf(
                        "external.installed",
                        &[("name", &m.name), ("version", &m.version)],
                    ),
                    &trf("external.installed_perms", &[("perms", &perms)]),
                    false,
                );
            }
            for p in old
                .plugins
                .iter()
                .filter(|p| !after.contains(&p.manifest.id))
            {
                tracing::info!(plugin = %p.manifest.name, "plugin externo quitado");
                self.system_notify(
                    &trf("external.removed", &[("name", &p.manifest.name)]),
                    "",
                    false,
                );
            }
            self.external_errors(&new.errors);
            *old = new;
        }
        self.install_plugins(&config);
    }

    fn external_errors(&self, errors: &[String]) {
        for e in errors {
            tracing::warn!(error = %e, "plugin externo no válido");
        }
        if let Some(e) = errors.first() {
            self.notice(format!("{}: {e}", tr("external.bad_manifest")));
        }
    }

    fn workflow_hooks(self: &Arc<Self>) -> WorkflowHooks {
        let (a, b, c) = (
            Arc::downgrade(self),
            Arc::downgrade(self),
            Arc::downgrade(self),
        );
        WorkflowHooks {
            run_script: Arc::new(move |name: &str, args: &[String]| {
                let h = a
                    .upgrade()
                    .ok_or_else(|| "Rukes se está cerrando".to_owned())?;
                let p = h
                    .scripts_plugin
                    .lock()
                    .ok()
                    .and_then(|p| p.clone())
                    .ok_or_else(|| tr("workflows.scripts_off").to_owned())?;
                p.run_named(name, args)
            }),
            set_theme: Arc::new(move |name: &str| {
                b.upgrade()
                    .ok_or_else(|| "Rukes se está cerrando".to_owned())?
                    .set_theme(name)
            }),
            notify: Arc::new(move |title: &str, body: &str, error: bool| {
                if let Some(h) = c.upgrade() {
                    h.system_notify(title, body, error);
                }
            }),
        }
    }

    fn workflow_errors(&self, errors: &[String]) {
        for e in errors {
            tracing::warn!(error = %e, "workflow no válido");
        }
        if let Some(e) = errors.first() {
            self.notice(format!("{}: {e}", tr("error.workflows")));
        }
    }

    /// Cambia el tema hasta reiniciar o hasta que cambie la configuración (paso `theme`).
    fn set_theme(&self, name: &str) -> Result<(), String> {
        rukes_config::theme::load(name, &self.paths.themes_dir()).map_err(|e| e.to_string())?;
        self.inner().config.general.theme = name.to_owned();
        self.refresh_look(true);
        Ok(())
    }

    /// Ejecuta lo que pide un trigger o un atajo propio.
    fn run_target(self: &Arc<Self>, target: &Target) {
        let runner = self
            .workflows
            .lock()
            .ok()
            .and_then(|w| w.as_ref().map(|(_, r, _, _)| Arc::clone(r)));
        let script = |name: &str| -> Result<(), String> {
            let p = self
                .scripts_plugin
                .lock()
                .ok()
                .and_then(|p| p.clone())
                .ok_or_else(|| tr("workflows.scripts_off").to_owned())?;
            p.run_named(name, &[])
        };
        let result = match target {
            Target::Workflow(name) => match runner.as_ref().and_then(|r| r.store().find(name)) {
                Some(wf) => {
                    let name = wf.name.clone();
                    if runner.as_ref().is_some_and(|r| r.start(wf)) {
                        Ok(())
                    } else {
                        Err(format!("{name}: {}", tr("workflows.already_running")))
                    }
                }
                // Un nombre suelto puede ser un script.
                None => {
                    script(name).map_err(|_| trf("workflows.target_not_found", &[("name", name)]))
                }
            },
            Target::Script(name) => script(name),
        };
        if let Err(e) = result {
            tracing::warn!(?target, error = %e, "no se pudo ejecutar");
            self.system_notify("Rukes", &e, true);
        }
    }

    /// Registra los atajos propios de scripts y workflows (RF-162). Se liberan en pausa.
    fn refresh_hotkeys(self: &Arc<Self>) {
        let mut wanted: Vec<(String, Target, String)> = Vec::new();
        if let Some((_, r, _, _)) = self
            .workflows
            .lock()
            .ok()
            .as_deref()
            .and_then(Option::as_ref)
        {
            for w in r.store().workflows() {
                if let Some(h) = &w.hotkey {
                    wanted.push((h.clone(), Target::Workflow(w.id.clone()), w.name.clone()));
                }
            }
        }
        if let Some(p) = self.scripts_plugin.lock().ok().and_then(|p| p.clone()) {
            for (h, name) in p.hotkeys() {
                wanted.push((h, Target::Script(name.clone()), name));
            }
        }
        if self.paused.load(Ordering::SeqCst) {
            wanted.clear();
        }
        let mut set = self.extra_hotkeys.lock().unwrap_or_else(|p| p.into_inner());
        if set.is_none() && wanted.is_empty() {
            return;
        }
        if set.is_none() {
            let targets = Arc::clone(&self.hotkey_targets);
            let me = Arc::downgrade(self);
            match HotkeySet::spawn(move |i| {
                let t = targets.lock().ok().and_then(|t| t.get(i).cloned());
                if let (Some(t), Some(h)) = (t, me.upgrade()) {
                    tracing::info!(target = ?t, "atajo propio");
                    h.run_target(&t);
                }
            }) {
                Ok(s) => *set = Some(s),
                Err(e) => {
                    tracing::error!(%e, "no se pudo crear el hilo de atajos propios");
                    return;
                }
            }
        }
        let main = Hotkey::parse(&self.inner().config.general.hotkey).ok();
        let mut errors = Vec::new();
        let mut vks = Vec::new();
        let mut targets = Vec::new();
        for (text, target, name) in wanted {
            let parsed = Hotkey::parse(&text)
                .map_err(|e| e.to_string())
                .and_then(|h| {
                    if Some(&h) == main.as_ref() {
                        return Err(tr("error.hotkey_main").to_owned());
                    }
                    hotkey::to_vk(h.mods.ctrl, h.mods.alt, h.mods.shift, h.mods.win, &h.key)
                        .map_err(|e| e.to_string())
                });
            match parsed {
                Ok(vk) if !vks.contains(&vk) => {
                    vks.push(vk);
                    targets.push((target, name, text));
                }
                Ok(_) => errors.push(format!("{name} «{text}»: {}", tr("error.hotkey_repeated"))),
                Err(e) => errors.push(format!("{name} «{text}»: {e}")),
            }
        }
        *self
            .hotkey_targets
            .lock()
            .unwrap_or_else(|p| p.into_inner()) =
            targets.iter().map(|(t, _, _)| t.clone()).collect();
        if let Some(s) = set.as_ref() {
            match s.set_all(vks) {
                Ok(results) => {
                    for (r, (_, name, text)) in results.into_iter().zip(&targets) {
                        match r {
                            Ok(()) => {
                                tracing::info!(hotkey = %text, %name, "atajo propio registrado")
                            }
                            Err(e) => errors.push(format!("{name} «{text}»: {e}")),
                        }
                    }
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        drop(set);
        for e in &errors {
            tracing::warn!(error = %e, "atajo propio no registrado");
        }
        if let Some(e) = errors.first() {
            self.notice(format!("{}: {e}", tr("error.hotkey")));
        }
    }

    /// Portapapeles, snippets, notas, tareas y recordatorios (secciones 5.10 a 5.12).
    fn text_plugins(self: &Arc<Self>, config: &Config, plugins: &mut Vec<Arc<dyn Plugin>>) {
        let c = &config.plugins.clipboard;
        let mut clip = self.clip.lock().unwrap_or_else(|p| p.into_inner());
        if c.enabled {
            let s = settings::clipboard(config);
            let history = match clip.as_ref() {
                Some((h, _)) => {
                    h.set_settings(s);
                    Some(Arc::clone(h))
                }
                None => {
                    let h = ClipHistory::open(self.paths.clipboard_dir().join("history.bin"), s);
                    let rec = Arc::downgrade(&h);
                    let watcher = ClipboardWatcher::spawn(move |change| match change {
                        Change::Text(c) => {
                            if let Some(h) = rec.upgrade() {
                                h.record(&c.text, c.source.as_deref());
                            }
                        }
                        Change::Sensitive => tracing::debug!("copia sensible ignorada"),
                        Change::Other => {}
                    });
                    match watcher {
                        Ok(w) => {
                            *clip = Some((Arc::clone(&h), w));
                            Some(h)
                        }
                        Err(e) => {
                            tracing::warn!(%e, "no se pudo escuchar el portapapeles");
                            None
                        }
                    }
                }
            };
            if let Some(history) = history {
                plugins.push(Arc::new(ClipboardPlugin {
                    history,
                    keyword: c.keyword.clone(),
                }));
            }
        } else if let Some((h, watcher)) = clip.take() {
            drop(watcher);
            h.save_now();
        }
        drop(clip);

        let sn = &config.plugins.snippets;
        let mut slot = self.snippets.lock().unwrap_or_else(|p| p.into_inner());
        if sn.enabled {
            let dir = settings::snippets_dir(config, &self.paths);
            let store = match slot.as_ref() {
                Some((d, s, _)) if *d == dir => Arc::clone(s),
                _ => {
                    let (store, errors) = SnippetStore::open(dir.clone());
                    self.snippet_errors(&errors);
                    let (weak, me) = (Arc::downgrade(&store), Arc::downgrade(self));
                    let watcher = DirWatcher::spawn_ext(&dir, &["toml", "md"], move || {
                        if let (Some(s), Some(h)) = (weak.upgrade(), me.upgrade()) {
                            h.snippet_errors(&s.reload());
                        }
                    })
                    .map_err(|e| tracing::warn!(%e, "no se pudo vigilar la carpeta de snippets"))
                    .ok();
                    *slot = Some((dir, Arc::clone(&store), watcher));
                    store
                }
            };
            plugins.push(Arc::new(SnippetsPlugin {
                store,
                keyword: sn.keyword.clone(),
                paste: sn.paste,
            }));
        } else {
            *slot = None;
        }
        drop(slot);

        let mut reminders = self.reminders.lock().unwrap_or_else(|p| p.into_inner());
        if config.plugins.notes.enabled {
            let (notes, todo) = settings::notes_files(config, &self.paths);
            plugins.push(Arc::new(NotesPlugin { file: notes }));
            plugins.push(Arc::new(TodoPlugin { file: todo }));
            let store = reminders.get_or_insert_with(|| {
                let s = RemindStore::open(self.paths.reminders_file());
                // Con la bandeja ya creada se avisa por ella; si no, `start_tray` lo conecta.
                if self.tray.lock().is_ok_and(|t| t.is_some()) {
                    self.connect_reminders(&s);
                }
                s
            });
            plugins.push(Arc::new(RemindPlugin {
                store: Arc::clone(store),
            }));
        } else {
            *reminders = None;
        }
    }

    fn connect_reminders(self: &Arc<Self>, store: &RemindStore) {
        let me = Arc::downgrade(self);
        store.set_notifier(Arc::new(move |title: &str, body: &str| {
            if let Some(h) = me.upgrade() {
                h.system_notify(title, body, false);
            }
        }));
    }

    fn snippet_errors(&self, errors: &[String]) {
        for e in errors {
            tracing::warn!(error = %e, "snippet no válido");
        }
        if let Some(e) = errors.first() {
            self.notice(format!("{}: {e}", tr("error.snippets")));
        }
    }

    /// Plugin de scripts (sección 5.13). El catálogo y su watcher se conservan entre recargas si
    /// la carpeta y los intérpretes no cambian.
    fn scripts_plugin(self: &Arc<Self>, config: &Config) -> Scripts {
        let settings = settings::scripts(config);
        let dir = settings::scripts_dir(config, &self.paths);
        let catalog = {
            let mut slot = self.scripts.lock().unwrap_or_else(|p| p.into_inner());
            match slot.as_ref() {
                Some((d, i, c)) if *d == dir && *i == settings.interpreters => Arc::clone(c),
                _ => {
                    let engine = Arc::downgrade(&self.engine);
                    let me = Arc::downgrade(self);
                    let c = Catalog::open(
                        dir.clone(),
                        settings.interpreters.clone(),
                        Arc::new(move || {
                            if let Some(e) = engine.upgrade() {
                                e.reload_metadata();
                            }
                            if let Some(h) = me.upgrade() {
                                h.refresh_hotkeys();
                            }
                        }),
                    );
                    *slot = Some((dir, settings.interpreters.clone(), Arc::clone(&c)));
                    c
                }
            }
        };
        let runs = {
            let mut slot = self.runs.lock().unwrap_or_else(|p| p.into_inner());
            let runs = slot.get_or_insert_with(|| {
                Arc::new(Runs::open(
                    self.paths.state_dir.join("script-runs.json"),
                    config.scripts.history_size,
                ))
            });
            runs.set_cap(config.scripts.history_size);
            Arc::clone(runs)
        };
        let (n, r, c) = (
            Arc::downgrade(self),
            Arc::downgrade(self),
            Arc::downgrade(self),
        );
        let hooks = Hooks {
            notify: Arc::new(move |title: &str, body: &str, error: bool| {
                if let Some(h) = n.upgrade() {
                    h.system_notify(title, body, error);
                }
            }),
            results: Arc::new(move |key: String, items: Vec<ResultItem>| {
                if let Some(h) = r.upgrade() {
                    h.script_results(key, items);
                }
            }),
            context: Arc::new(move || c.upgrade().map(|h| h.context()).unwrap_or_default()),
        };
        let plugin = Arc::new(ScriptsPlugin::new(catalog, runs, settings, hooks));
        *self
            .scripts_plugin
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(Arc::clone(&plugin));
        Scripts(plugin)
    }

    /// Notificación del sistema desde el icono de la bandeja.
    fn system_notify(&self, title: &str, body: &str, error: bool) {
        match self.tray.lock().ok().as_deref() {
            Some(Some(t)) => t.notify(title, body, error),
            _ => self.notice(format!("{title}: {body}")),
        }
    }

    /// Salida de un script en modo launcher: a la lista si la ventana está visible; si no, como
    /// notificación.
    fn script_results(&self, key: String, items: Vec<ResultItem>) {
        if window::is_visible(self.hwnd.load(Ordering::SeqCst)) {
            let _ = self.ui.unbounded_send(UiEvent::Results {
                replaces: key,
                items,
            });
        } else if let Some(first) = items.first() {
            let body = items
                .get(1)
                .map_or(first.subtitle.as_str(), |i| i.title.as_str())
                .to_owned();
            self.system_notify(&first.title, &body, false);
        }
    }

    /// Ventana activa antes de abrir el launcher y su selección en el Explorador (RF-141).
    fn context(&self) -> RunContext {
        let prev = self.previous.load(Ordering::SeqCst);
        let w = rukes_platform::explorer::window_context(prev);
        let selected = if w.class == "CabinetWClass" || w.class == "ExploreWClass" {
            rukes_platform::explorer::selection(prev)
        } else {
            Vec::new()
        };
        RunContext {
            active_window: w.title,
            active_exe: w
                .exe
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            selected,
        }
    }

    /// Registra el atajo global; si falla, avisa en la UI (y en el log).
    pub fn start_hotkey(self: &Arc<Self>, text: &str) {
        let mut inner = self.inner();
        if inner.hotkey.is_none() {
            let weak = Arc::downgrade(self);
            match HotkeyManager::spawn(move || {
                if let Some(h) = weak.upgrade() {
                    h.toggle(false);
                }
            }) {
                Ok(m) => inner.hotkey = Some(m),
                Err(e) => {
                    tracing::error!(%e, "no se pudo crear el hilo del atajo");
                    return;
                }
            }
        }
        if inner.hotkey_text == text {
            return;
        }
        let result = Hotkey::parse(text)
            .map_err(|e| e.to_string())
            .and_then(|h| {
                hotkey::to_vk(h.mods.ctrl, h.mods.alt, h.mods.shift, h.mods.win, &h.key)
                    .map_err(|e| e.to_string())
            })
            .and_then(|vk| {
                inner
                    .hotkey
                    .as_ref()
                    .map_or(Ok(()), |m| m.set(vk).map_err(|e| e.to_string()))
            });
        match result {
            Ok(()) => {
                tracing::info!(hotkey = text, "atajo registrado");
                inner.hotkey_text = text.to_owned();
            }
            Err(e) => {
                tracing::error!(hotkey = text, error = %e, "no se pudo registrar el atajo");
                drop(inner);
                self.notice(format!("{} «{text}»: {e}", tr("error.hotkey")));
            }
        }
    }

    /// Muestra u oculta la ventana. Se llama desde el hilo del atajo (con permiso de primer
    /// plano) o desde el de instancia única (`force_show`).
    pub fn toggle(&self, force_show: bool) {
        let hwnd = self.hwnd.load(Ordering::SeqCst);
        if hwnd == 0 {
            return;
        }
        if !force_show && window::is_visible(hwnd) && window::is_foreground(hwnd) {
            self.hide_now();
            return;
        }
        let t = Instant::now();
        self.hide_seq.fetch_add(1, Ordering::SeqCst);
        // Modo claro/oscuro, contraste alto o escalado pudieron cambiar con la ventana oculta.
        self.refresh_look(false);
        let (placement, ref_height) = {
            let inner = self.inner();
            (
                settings::placement(&inner.config),
                settings::reference_height(&inner.config, &inner.style),
            )
        };
        window::place_on_cursor_monitor(hwnd, placement, ref_height);
        let previous = window::show_and_focus(hwnd);
        self.previous.store(previous, Ordering::SeqCst);
        *self.shown_at.lock().unwrap_or_else(|p| p.into_inner()) = Some(t);
        let _ = self.ui.unbounded_send(UiEvent::Shown);
    }

    /// Oculta, devuelve el foco a la app anterior y, si sigue oculta un rato, libera memoria.
    fn hide_now(&self) {
        let hwnd = self.hwnd.load(Ordering::SeqCst);
        if hwnd == 0 {
            return;
        }
        window::hide_and_restore(hwnd, self.previous.load(Ordering::SeqCst));
        let seq = self.hide_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let me = self.me.lock().ok().and_then(|w| w.upgrade());
        if let Some(me) = me {
            let _ = std::thread::Builder::new()
                .name("rukes-trim".into())
                .spawn(move || {
                    std::thread::sleep(TRIM_AFTER);
                    let hwnd = me.hwnd.load(Ordering::SeqCst);
                    if me.hide_seq.load(Ordering::SeqCst) == seq && !window::is_visible(hwnd) {
                        rukes_platform::sysinfo::trim_working_set();
                        let (ws, private) = rukes_platform::sysinfo::memory_mb();
                        tracing::debug!(
                            working_set_mb = ws,
                            private_mb = private,
                            "memoria recortada"
                        );
                    }
                });
        }
    }

    /// Aplica una configuración recargada en caliente (RF-201).
    pub fn apply_config(&self, res: Result<Config, ConfigError>) {
        let config = match res {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(%e, "configuración no válida; se mantiene la anterior");
                self.notice(format!("{}: {e}", tr("error.config")));
                return;
            }
        };
        tracing::info!("configuración recargada");
        if let Err(e) = rukes_core::i18n::set_language(
            &config.general.language,
            &rukes_platform::sysinfo::user_locale(),
        ) {
            self.notice(format!("{}: {e}", tr("error.lang")));
        }
        self.engine.set_config(settings::engine(&config));
        let old = std::mem::replace(&mut self.inner().config, config.clone());
        self.refresh_look(true);
        if old.plugins != config.plugins
            || old.scripts != config.scripts
            || old.workflows != config.workflows
        {
            let me = self.me.lock().ok().and_then(|w| w.upgrade());
            if let Some(me) = me {
                me.install_plugins(&config);
                if config.plugins.apps.enabled && !old.plugins.apps.enabled {
                    self.engine.refresh_all();
                }
            }
        }
        if old.general.start_with_windows != config.general.start_with_windows {
            apply_autostart(&config);
        }
        if old.general.hotkey != config.general.hotkey && !self.paused.load(Ordering::SeqCst) {
            let me = self.me.lock().ok().and_then(|w| w.upgrade());
            if let Some(me) = me {
                me.start_hotkey(&config.general.hotkey);
            }
        }
    }

    /// Crea el icono de la bandeja (RF-007).
    pub fn start_tray(self: &Arc<Self>) {
        let menu_host = Arc::downgrade(self);
        let click_host = Arc::downgrade(self);
        let cmd_host = Arc::downgrade(self);
        let tray = Tray::spawn(
            "Rukes",
            move || {
                let Some(h) = menu_host.upgrade() else {
                    return Vec::new();
                };
                let paused = h.paused.load(Ordering::SeqCst);
                let hotkey = h.inner().config.general.hotkey.clone();
                let triggers = h.triggers_paused();
                let mut menu = vec![
                    MenuEntry::Item(TRAY_SHOW, format!("{} ({hotkey})", tr("tray.show"))),
                    MenuEntry::Item(TRAY_SETTINGS, tr("tray.settings").to_owned()),
                    MenuEntry::Item(TRAY_RELOAD, tr("tray.reload").to_owned()),
                    MenuEntry::Item(
                        TRAY_PAUSE,
                        tr(if paused { "tray.resume" } else { "tray.pause" }).to_owned(),
                    ),
                ];
                if let Some(t) = triggers {
                    menu.push(MenuEntry::Item(
                        TRAY_TRIGGERS,
                        tr(if t {
                            "tray.resume_triggers"
                        } else {
                            "tray.pause_triggers"
                        })
                        .to_owned(),
                    ));
                }
                menu.push(MenuEntry::Separator);
                menu.push(MenuEntry::Item(TRAY_QUIT, tr("tray.quit").to_owned()));
                menu
            },
            move || {
                if let Some(h) = click_host.upgrade() {
                    h.toggle(true);
                }
            },
            move |id| {
                if let Some(h) = cmd_host.upgrade() {
                    h.tray_command(id);
                }
            },
        );
        match tray {
            Ok(t) => *self.tray.lock().unwrap_or_else(|p| p.into_inner()) = Some(t),
            Err(e) => tracing::warn!(%e, "no se pudo crear el icono de la bandeja"),
        }
        // Los recordatorios vencidos con Rukes cerrado avisan ahora (RF-123).
        let store = self
            .reminders
            .lock()
            .ok()
            .and_then(|r| r.as_ref().map(Arc::clone));
        if let Some(s) = store {
            self.connect_reminders(&s);
        }
    }

    fn tray_command(self: &Arc<Self>, id: u16) {
        match id {
            TRAY_SHOW => self.toggle(true),
            TRAY_SETTINGS => {
                if let Err(e) = crate::system_plugin::open_config(&self.paths.config_file()) {
                    self.notice(format!("{}: {e}", tr("error.action")));
                }
            }
            TRAY_RELOAD => {
                self.inner().last_refresh = Instant::now();
                if let Some(f) = self.files.lock().ok().and_then(|f| f.clone()) {
                    f.rescan(true);
                }
                self.engine.refresh_all();
            }
            TRAY_PAUSE => self.set_paused(!self.paused.load(Ordering::SeqCst)),
            TRAY_QUIT => self.quit(),
            TRAY_TRIGGERS => {
                if let Some(s) = self.scheduler() {
                    let paused = !s.paused();
                    s.set_paused(paused);
                    self.system_notify(
                        "Rukes",
                        tr(if paused {
                            "tray.triggers_paused"
                        } else {
                            "tray.triggers_resumed"
                        }),
                        false,
                    );
                }
            }
            _ => {}
        }
    }

    fn scheduler(&self) -> Option<Arc<Scheduler>> {
        self.workflows
            .lock()
            .ok()
            .and_then(|w| w.as_ref().map(|(_, _, s, _)| Arc::clone(s)))
    }

    /// `Some(en pausa)` si hay triggers.
    fn triggers_paused(&self) -> Option<bool> {
        self.scheduler().map(|s| s.paused())
    }

    /// Pausa: libera el atajo global (y los propios) hasta reanudar.
    pub fn set_paused(self: &Arc<Self>, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        if paused {
            let mut inner = self.inner();
            inner.hotkey = None;
            inner.hotkey_text.clear();
            tracing::info!("atajo en pausa");
        } else {
            let text = self.inner().config.general.hotkey.clone();
            self.start_hotkey(&text);
        }
        self.refresh_hotkeys();
    }

    /// Guarda el estado, quita el icono de la bandeja y termina el proceso.
    pub fn quit(&self) {
        tracing::info!("salida pedida por el usuario");
        self.shutdown();
        if let Some(t) = self.tray.lock().ok().and_then(|mut t| t.take()) {
            t.remove_now();
        }
        std::process::exit(0);
    }

    pub fn shutdown(&self) {
        let clip = self
            .clip
            .lock()
            .ok()
            .and_then(|c| c.as_ref().map(|(h, _)| Arc::clone(h)));
        if let Some(h) = clip {
            h.save_now();
        }
        if let Err(e) = self.engine.save_frecency(&self.paths.frecency_file()) {
            tracing::warn!(%e, "no se pudo guardar la frecency al salir");
        }
    }
}

impl Host for AppHost {
    fn query(&self, text: &str, sink: Sink) -> QueryId {
        self.engine.query(text, sink)
    }

    fn execute(
        &self,
        query: &str,
        item: &ResultItem,
        action: &str,
    ) -> Result<ActionOutcome, String> {
        self.engine
            .execute(query, item, action)
            .map_err(|e| e.to_string())
    }

    fn window_ready(&self, hwnd: isize) -> bool {
        self.hwnd.store(hwnd, Ordering::SeqCst);
        let mut inner = self.inner();
        inner.dwm = None;
        self.apply_dwm(&mut inner);
        let translucent = inner
            .dwm
            .is_some_and(|(_, applied)| applied != dwm::Backdrop::Solid);
        inner.style.translucent = translucent;
        tracing::info!(translucent, "ventana lista");
        // Un `Config` anterior (enviado antes de existir la ventana) diría «sólido».
        let ui = settings::ui(
            &inner.config,
            inner.style.clone(),
            inner.system.animations,
            inner.system.text_scale,
        );
        drop(inner);
        let _ = self.ui.unbounded_send(UiEvent::Config(ui));
        translucent
    }

    fn hide(&self) {
        self.hide_now();
    }

    /// Pega en la ventana anterior (RF-103, RF-112). La UI ya se ocultó y devolvió el foco.
    fn paste(&self, text: &str, cursor_left: usize, restore: bool) {
        use rukes_platform::clipboard;
        let text = text.to_owned();
        let spawned = std::thread::Builder::new()
            .name("rukes-paste".into())
            .spawn(move || {
                let old = if restore {
                    clipboard::get_text(rukes_plugins::clipboard::MAX_CHARS)
                } else {
                    None
                };
                // Los snippets no deben acabar en el historial del portapapeles.
                let set = if restore {
                    clipboard::set_text_private(&text)
                } else {
                    clipboard::set_text(&text)
                };
                if let Err(e) = set {
                    tracing::warn!(%e, "no se pudo preparar el texto para pegar");
                    return;
                }
                std::thread::sleep(PASTE_DELAY);
                if !rukes_platform::input::paste(cursor_left) {
                    tracing::warn!("Windows bloqueó el pegado (¿ventana de administrador?)");
                }
                if let Some(old) = old {
                    std::thread::sleep(RESTORE_DELAY);
                    let _ = clipboard::set_text_private(&old);
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(%e, "no se pudo pegar");
        }
    }

    fn icon(&self, icon: &Icon, px: u32) -> Option<IconRgba> {
        let (source, thumb) = match icon {
            Icon::Shell(s) => (s.clone(), false),
            Icon::File(p) => (p.to_string_lossy().into_owned(), false),
            Icon::Thumbnail(p) => (p.to_string_lossy().into_owned(), true),
            Icon::None | Icon::Named(_) => return None,
        };
        // Las miniaturas se guardan aparte de los iconos del mismo archivo.
        let key = IconCache::key(&source, px) ^ if thumb { 0x7468_756d_6200 } else { 0 };
        if let Some(i) = self.icon_cache.get(key) {
            return Some(i);
        }
        let i = thumb
            .then(|| icons::thumbnail(&source, px))
            .flatten()
            .or_else(|| icons::load(&source, px))
            .map(|i| (i.width, i.height, i.pixels))?;
        self.icon_cache.put(key, &i);
        Some(i)
    }

    fn preview(&self, path: &std::path::Path) -> Option<Preview> {
        crate::preview::build(path)
    }

    fn shown(&self) {
        if let Some(t) = self.shown_at.lock().ok().and_then(|mut t| t.take()) {
            tracing::debug!(us = t.elapsed().as_micros() as u64, "atajo → UI lista");
        }
        let stale = {
            let mut inner = self.inner();
            let stale = inner.last_refresh.elapsed() > REINDEX_AFTER;
            if stale {
                inner.last_refresh = Instant::now();
            }
            stale
        };
        if stale {
            self.engine.refresh_all();
        }
    }
}
