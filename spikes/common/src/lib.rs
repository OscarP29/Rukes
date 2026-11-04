//! Arnés compartido por los spikes de UI: efectos DWM, medición de memoria y un
//! benchmark automático idéntico para cada motor (ocultar/mostrar, fade-in, animación).
//!
//! Protocolo del benchmark (hilo de control, independiente del motor):
//! 1. Espera el primer frame → `cold_start_first_frame_ms` (desde la creación del proceso).
//! 2. 10 ciclos: `ShowWindow(SW_HIDE)`, espera, `ShowWindow(SW_SHOW)` + foco; mide el tiempo
//!    hasta que el motor reporta el siguiente frame (`show_latency_ms`).
//! 3. Mide memoria con la ventana oculta 3 s.
//! 4. 3 s de animación continua de selección: mide intervalos entre frames.
//! 5. Mide memoria visible, escribe `results/<motor>.json` y termina el proceso.
//!
//! Variables: `SPIKE_NAME` (nombre del resultado), `SPIKE_RESULTS` (carpeta),
//! `SPIKE_STAY=1` (no salir al terminar, para inspección visual), `SPIKE_NOBENCH=1`.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{FILETIME, HWND};
use windows::Win32::Graphics::Dwm::{
    DWMSBT_MAINWINDOW, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetWindowLongPtrW, SW_HIDE, SW_SHOW, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_CAPTION,
    WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
};

/// Número de filas del spike.
pub const ROWS: usize = 8;
/// Duración del fade-in en ms.
pub const FADE_MS: f32 = 120.0;
/// Ancho lógico de la ventana.
pub const WIDTH: f32 = 680.0;
/// Alto del campo de búsqueda.
pub const SEARCH_H: f32 = 60.0;
/// Alto de fila.
pub const ROW_H: f32 = 48.0;
/// Alto lógico: campo de búsqueda + 8 filas + margen.
pub const HEIGHT: f32 = SEARCH_H + ROWS as f32 * ROW_H + 12.0;

/// Colores (Tokyo Night) en RGB 0..1.
pub mod color {
    /// Texto principal.
    pub const FG: [f32; 3] = [0.753, 0.792, 0.961];
    /// Texto secundario.
    pub const MUTED: [f32; 3] = [0.337, 0.373, 0.537];
    /// Fondo de la fila seleccionada.
    pub const SELECTION: [f32; 3] = [0.157, 0.204, 0.341];
    /// Acento.
    pub const ACCENT: [f32; 3] = [0.478, 0.635, 0.969];
    /// Tinte del fondo encima de Mica (con alfa).
    pub const TINT: [f32; 4] = [0.102, 0.106, 0.149, 0.55];
}

/// Títulos de ejemplo de las filas.
pub const ITEMS: [(&str, &str); ROWS] = [
    ("Google Chrome", "Aplicación · C:\\Program Files\\Google\\Chrome"),
    ("Visual Studio Code", "Aplicación · Ctrl+Enter abrir como admin"),
    ("Windows Terminal", "Aplicación · wt.exe"),
    ("= 5 km a millas", "3,10686 mi · Enter copia"),
    ("> shell:startup", "Abrir carpeta de inicio"),
    ("Crear issue en GitHub", "Script · gh issue <título>"),
    ("Modo trabajo", "Workflow · 4 pasos"),
    ("kill chrome", "Proceso · 412 MB · 6 instancias"),
];

/// Tipo de fondo DWM.
#[derive(Clone, Copy, Debug)]
pub enum Backdrop {
    /// Mica (`DWMSBT_MAINWINDOW`).
    Mica,
    /// Acrílico (`DWMSBT_TRANSIENTWINDOW`).
    Acrylic,
}

/// Fondo elegido por `SPIKE_BACKDROP` (`mica` por defecto, `acrylic`).
pub fn backdrop_from_env() -> Backdrop {
    match std::env::var("SPIKE_BACKDROP").as_deref() {
        Ok("acrylic") => Backdrop::Acrylic,
        _ => Backdrop::Mica,
    }
}

/// Aplica modo oscuro, esquinas redondeadas y Mica/Acrílico a la ventana.
pub fn apply_backdrop(hwnd: isize, kind: Backdrop) {
    let hwnd = HWND(hwnd as *mut _);
    let dark: i32 = 1;
    let corner = DWMWCP_ROUND;
    let backdrop = match kind {
        Backdrop::Mica => DWMSBT_MAINWINDOW,
        Backdrop::Acrylic => DWMSBT_TRANSIENTWINDOW,
    };
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    // SAFETY: hwnd es una ventana viva de este proceso y los punteros apuntan a valores
    // locales de 4 bytes, que es el tamaño indicado.
    unsafe {
        // Sin estilos de marco: si no, DWM pinta los botones de título al extender el marco.
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let strip = (WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME).0 as isize;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style & !strip);
        let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const dark).cast(),
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&raw const corner).cast(),
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&raw const backdrop).cast(),
            4,
        );
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

/// Memoria del proceso en MB.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mem {
    /// Working set (columna «Memoria» del Administrador de tareas aprox.).
    pub working_set_mb: f64,
    /// Bytes privados confirmados.
    pub private_mb: f64,
}

/// Lee la memoria actual del proceso.
pub fn mem() -> Mem {
    let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
    // SAFETY: estructura local con el tamaño correcto; el pseudo-handle del proceso es válido.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&raw mut c).cast(),
            size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
    };
    if ok.is_err() {
        return Mem::default();
    }
    Mem {
        working_set_mb: c.WorkingSetSize as f64 / 1_048_576.0,
        private_mb: c.PrivateUsage as f64 / 1_048_576.0,
    }
}

fn ft(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

/// Milisegundos desde la creación del proceso.
pub fn process_age_ms() -> f64 {
    let mut c = FILETIME::default();
    let mut e = FILETIME::default();
    let mut k = FILETIME::default();
    let mut u = FILETIME::default();
    // SAFETY: punteros a FILETIME locales; pseudo-handle válido.
    unsafe {
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).is_err() {
            return f64::NAN;
        }
        let now = GetSystemTimePreciseAsFileTime();
        ft(now).saturating_sub(ft(c)) as f64 / 10_000.0
    }
}

// ---------------------------------------------------------------------------------------------
// Estado compartido entre el motor y el hilo de control.

static HWND_SLOT: AtomicIsize = AtomicIsize::new(0);
static ANIMATING: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<State>> = OnceLock::new();

type Notify = Box<dyn Fn() + Send>;

#[derive(Default)]
struct State {
    first_frame_ms: Option<f64>,
    pending_show: Option<Instant>,
    shown_at: Option<Instant>,
    anim_start: Option<Instant>,
    show_latencies: Vec<f64>,
    frame_times: Vec<Instant>,
    notify: Option<Notify>,
}

fn state() -> MutexGuard<'static, State> {
    STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

/// El motor llama a esto en cuanto tiene el HWND. `notify` debe pedir un repintado al motor
/// (se invoca desde el hilo de control).
pub fn register(hwnd: isize, notify: impl Fn() + Send + 'static) {
    HWND_SLOT.store(hwnd, Ordering::SeqCst);
    state().notify = Some(Box::new(notify));
}

/// `true` si ya se registró el HWND.
pub fn registered() -> bool {
    HWND_SLOT.load(Ordering::SeqCst) != 0
}

/// El motor llama a esto al terminar cada frame.
pub fn frame_rendered() {
    let now = Instant::now();
    let mut s = state();
    if s.first_frame_ms.is_none() {
        s.first_frame_ms = Some(process_age_ms());
        drop(s);
        if std::env::var_os("SPIKE_NOBENCH").is_none() {
            std::thread::spawn(control_thread);
        }
        return;
    }
    if let Some(t) = s.pending_show.take() {
        s.show_latencies.push(now.duration_since(t).as_secs_f64() * 1000.0);
    }
    if ANIMATING.load(Ordering::Relaxed) {
        s.frame_times.push(now);
    }
}

/// Opacidad del contenido (0..1) para el fade-in tras el último `show`.
pub fn fade_alpha() -> f32 {
    match state().shown_at {
        Some(t) => (t.elapsed().as_secs_f32() * 1000.0 / FADE_MS).clamp(0.0, 1.0),
        None => 1.0,
    }
}

/// `true` mientras el fade-in o la animación de prueba necesitan frames continuos.
pub fn needs_animation() -> bool {
    ANIMATING.load(Ordering::Relaxed) || fade_alpha() < 1.0
}

/// Posición de la selección (en filas, con decimales). Durante la prueba de fluidez recorre
/// las filas de ida y vuelta cada 2 s.
pub fn selection_pos() -> f32 {
    if !ANIMATING.load(Ordering::Relaxed) {
        return 0.0;
    }
    let Some(start) = state().anim_start else {
        return 0.0;
    };
    let t = start.elapsed().as_secs_f32();
    let p = 0.5 - (t * std::f32::consts::PI).cos() * 0.5;
    p * (ROWS as f32 - 1.0)
}

fn notify() {
    if let Some(n) = state().notify.as_ref() {
        n();
    }
}

fn show(hwnd: HWND, visible: bool) {
    // SAFETY: hwnd es la ventana del motor, viva durante todo el benchmark.
    unsafe {
        let _ = ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
        if visible {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn control_thread() {
    let name = std::env::var("SPIKE_NAME").unwrap_or_else(|_| "spike".into());
    let hwnd = HWND(HWND_SLOT.load(Ordering::SeqCst) as *mut _);
    std::thread::sleep(Duration::from_millis(1000));
    let mem_first = mem();

    for _ in 0..10 {
        show(hwnd, false);
        std::thread::sleep(Duration::from_millis(300));
        {
            let mut s = state();
            let now = Instant::now();
            s.pending_show = Some(now);
            s.shown_at = Some(now);
        }
        show(hwnd, true);
        notify();
        std::thread::sleep(Duration::from_millis(400));
    }

    show(hwnd, false);
    std::thread::sleep(Duration::from_millis(3000));
    let mem_hidden = mem();
    show(hwnd, true);
    std::thread::sleep(Duration::from_millis(300));

    state().anim_start = Some(Instant::now());
    ANIMATING.store(true, Ordering::SeqCst);
    notify();
    std::thread::sleep(Duration::from_millis(3000));
    ANIMATING.store(false, Ordering::SeqCst);
    let mem_visible = mem();

    let s = state();
    let mut lat = s.show_latencies.clone();
    lat.sort_by(f64::total_cmp);
    let p = |q: f64| {
        lat.get(((lat.len() as f64 - 1.0) * q).round() as usize)
            .copied()
            .unwrap_or(f64::NAN)
    };
    let gaps: Vec<f64> = s
        .frame_times
        .windows(2)
        .map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0)
        .collect();
    let anim_secs = match (s.frame_times.first(), s.frame_times.last()) {
        (Some(a), Some(b)) => (*b - *a).as_secs_f64(),
        _ => f64::NAN,
    };
    let fps = s.frame_times.len().saturating_sub(1) as f64 / anim_secs;
    let max_gap = gaps.iter().copied().fold(0.0, f64::max);
    let over_25 = gaps.iter().filter(|g| **g > 25.0).count();
    let exe_mb = std::env::current_exe()
        .and_then(std::fs::metadata)
        .map(|m| m.len() as f64 / 1_048_576.0)
        .unwrap_or(f64::NAN);

    let json = format!(
        "{{\n  \"engine\": \"{name}\",\n  \"exe_mb\": {exe_mb:.2},\n  \"cold_start_first_frame_ms\": {:.1},\n  \"show_latency_ms\": {{ \"p50\": {:.1}, \"p95\": {:.1}, \"max\": {:.1}, \"samples\": {} }},\n  \"mem_after_start_mb\": {{ \"working_set\": {:.1}, \"private\": {:.1} }},\n  \"mem_hidden_mb\": {{ \"working_set\": {:.1}, \"private\": {:.1} }},\n  \"mem_visible_mb\": {{ \"working_set\": {:.1}, \"private\": {:.1} }},\n  \"animation\": {{ \"fps\": {fps:.1}, \"max_frame_gap_ms\": {max_gap:.1}, \"frames_over_25ms\": {over_25}, \"frames\": {} }}\n}}\n",
        s.first_frame_ms.unwrap_or(f64::NAN),
        p(0.5),
        p(0.95),
        lat.last().copied().unwrap_or(f64::NAN),
        lat.len(),
        mem_first.working_set_mb,
        mem_first.private_mb,
        mem_hidden.working_set_mb,
        mem_hidden.private_mb,
        mem_visible.working_set_mb,
        mem_visible.private_mb,
        s.frame_times.len(),
    );
    drop(s);
    print!("{json}");
    let dir = std::env::var("SPIKE_RESULTS").unwrap_or_else(|_| "results".into());
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(format!("{dir}/{name}.json"), json);
    if std::env::var_os("SPIKE_STAY").is_none() {
        std::process::exit(0);
    }
}
