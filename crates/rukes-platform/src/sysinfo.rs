//! Información del proceso para las métricas de la sección 6.1.

use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::GetCurrentProcess;

/// Memoria del proceso en MB: `(working set, privada)`.
pub fn memory_mb() -> (f64, f64) {
    let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
    // SAFETY: estructura local con su tamaño; pseudo-handle del proceso actual.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&raw mut c).cast(),
            size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        )
    };
    if ok.is_err() {
        return (0.0, 0.0);
    }
    (
        c.WorkingSetSize as f64 / 1_048_576.0,
        c.PrivateUsage as f64 / 1_048_576.0,
    )
}

/// Milisegundos desde la creación del proceso (para medir el arranque, RNF-005).
pub fn process_age_ms() -> f64 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
    use windows::Win32::System::Threading::GetProcessTimes;
    let ft = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: punteros a FILETIME locales; pseudo-handle del proceso actual.
    unsafe {
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).is_err() {
            return f64::NAN;
        }
        ft(GetSystemTimePreciseAsFileTime()).saturating_sub(ft(c)) as f64 / 10_000.0
    }
}

/// Configuración regional del usuario (p. ej. `es-CO`).
pub fn user_locale() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85];
    // SAFETY: buffer local del tamaño máximo documentado (LOCALE_NAME_MAX_LENGTH).
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n <= 1 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(n - 1) as usize])
}

/// Devuelve al sistema las páginas del working set (siguen en la lista de espera y vuelven con
/// fallos de página suaves si se necesitan). Se usa al ocultar la ventana (RNF-001).
pub fn trim_working_set() {
    use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
    // SAFETY: pseudo-handle del proceso actual, con permisos completos sobre sí mismo.
    let _ = unsafe { EmptyWorkingSet(GetCurrentProcess()) };
}

/// Segundos desde que arrancó Windows.
pub fn uptime_secs() -> u64 {
    // SAFETY: sin precondiciones.
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() / 1000 }
}
