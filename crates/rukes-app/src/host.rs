//! Orquestación: implementa [`rukes_ui::Host`] y conecta el motor, la plataforma y la config.

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use rukes_config::{Config, ConfigError, Hotkey};
use rukes_core::i18n::tr;
use rukes_core::{ActionOutcome, Engine, Icon, Plugin, QueryId, ResultItem};
use rukes_platform::hotkey::{self, HotkeyManager};
use rukes_platform::{dwm, icons, window};
use rukes_plugins::AppsPlugin;
use rukes_ui::{Host, IconRgba, Sink, UiEvent, UiSender};

use crate::settings;
use crate::system_plugin::SystemPlugin;

/// Antigüedad del índice de apps a partir de la cual se reindexa al mostrar la ventana.
const REINDEX_AFTER: Duration = Duration::from_secs(10 * 60);
/// Espera tras ocultar antes de liberar el working set (por si se vuelve a mostrar enseguida).
const TRIM_AFTER: Duration = Duration::from_millis(1500);

struct Inner {
    config: Config,
    hotkey: Option<HotkeyManager>,
    hotkey_text: String,
    last_refresh: Instant,
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
}

impl AppHost {
    pub fn new(
        engine: Arc<Engine>,
        ui: UiSender,
        paths: rukes_config::paths::Paths,
        config: &Config,
    ) -> Self {
        Self {
            engine,
            ui,
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
            }),
            me: Mutex::new(Weak::new()),
        }
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn notice(&self, text: String) {
        let _ = self.ui.unbounded_send(UiEvent::Notice(text));
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
        let quit_engine = Arc::clone(&self.engine);
        let frecency_file = self.paths.frecency_file();
        let reload_engine = Arc::clone(&self.engine);
        plugins.push(Arc::new(SystemPlugin {
            config_file: self.paths.config_file(),
            on_quit: Box::new(move || {
                let _ = quit_engine.save_frecency(&frecency_file);
                tracing::info!("salida pedida por el usuario");
                std::process::exit(0);
            }),
            on_reload: Box::new(move || reload_engine.refresh_all()),
        }));
        self.engine.set_plugins(plugins);
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
        let (placement, ref_height) = {
            let inner = self.inner();
            (
                settings::placement(&inner.config),
                settings::reference_height(&inner.config),
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
        rukes_core::i18n::set_language(
            &config.general.language,
            &rukes_platform::sysinfo::user_locale(),
        );
        self.engine.set_config(settings::engine(&config));
        let _ = self
            .ui
            .unbounded_send(UiEvent::Config(settings::ui(&config)));
        let old = std::mem::replace(&mut self.inner().config, config.clone());
        if old.window.backdrop != config.window.backdrop {
            let hwnd = self.hwnd.load(Ordering::SeqCst);
            if hwnd != 0 {
                dwm::apply(hwnd, settings::backdrop(&config), true);
            }
        }
        if old.plugins != config.plugins {
            let me = self.me.lock().ok().and_then(|w| w.upgrade());
            if let Some(me) = me {
                me.install_plugins(&config);
                if config.plugins.apps.enabled && !old.plugins.apps.enabled {
                    self.engine.refresh_all();
                }
            }
        }
        if old.general.hotkey != config.general.hotkey {
            let me = self.me.lock().ok().and_then(|w| w.upgrade());
            if let Some(me) = me {
                me.start_hotkey(&config.general.hotkey);
            }
        }
    }

    pub fn shutdown(&self) {
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
        let backdrop = settings::backdrop(&self.inner().config);
        let applied = dwm::apply(hwnd, backdrop, true);
        tracing::info!(?applied, "ventana lista");
        applied != dwm::Backdrop::Solid
    }

    fn hide(&self) {
        self.hide_now();
    }

    fn icon(&self, icon: &Icon, px: u32) -> Option<IconRgba> {
        let source = match icon {
            Icon::Shell(s) => s.clone(),
            Icon::File(p) => p.to_string_lossy().into_owned(),
            Icon::None | Icon::Named(_) => return None,
        };
        icons::load(&source, px).map(|i| (i.width, i.height, i.pixels))
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
