//! Instancia única (RF-006): un segundo proceso avisa al primero para que muestre la ventana.

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent,
    WaitForSingleObject,
};
use windows::core::PCWSTR;

use crate::{Context as _, Result, wide};

/// Resultado de intentar ser la instancia principal.
pub enum Instance {
    /// Somos la principal; mantener vivo mientras dure el proceso.
    Primary(PrimaryGuard),
    /// Ya había otra; se le pidió que se muestre.
    Secondary,
}

/// Mantiene el mutex de instancia única.
pub struct PrimaryGuard {
    mutex: HANDLE,
}

// SAFETY: un HANDLE de mutex del kernel puede usarse y cerrarse desde cualquier hilo.
unsafe impl Send for PrimaryGuard {}

impl Drop for PrimaryGuard {
    fn drop(&mut self) {
        // SAFETY: handle propio, se cierra una sola vez.
        let _ = unsafe { CloseHandle(self.mutex) };
    }
}

/// Intenta ser la instancia principal de `name` (por sesión de usuario). Si ya hay otra, le
/// envía la señal de «mostrar» y devuelve [`Instance::Secondary`].
///
/// En la principal, `on_show_request` se llama (en un hilo propio) cada vez que otro proceso
/// pide mostrar la ventana.
pub fn acquire(name: &str, on_show_request: impl Fn() + Send + 'static) -> Result<Instance> {
    let mutex_name = wide(&format!("Local\\{name}-instance"));
    let event_name = wide(&format!("Local\\{name}-show"));
    // SAFETY: nombres terminados en cero que viven durante la llamada.
    let mutex =
        unsafe { CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr())) }.ctx("CreateMutexW")?;
    // SAFETY: lectura del último error justo tras CreateMutexW.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: handle propio; nombre válido.
        unsafe {
            let _ = CloseHandle(mutex);
            if let Ok(ev) = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(event_name.as_ptr())) {
                let _ = SetEvent(ev);
                let _ = CloseHandle(ev);
            }
        }
        return Ok(Instance::Secondary);
    }
    // Evento de auto-reset.
    // SAFETY: nombre válido.
    let event = unsafe { CreateEventW(None, false, false, PCWSTR(event_name.as_ptr())) }
        .ctx("CreateEventW")?;
    let event_raw = event.0 as isize;
    std::thread::Builder::new()
        .name("rukes-instance".into())
        .spawn(move || {
            let ev = HANDLE(event_raw as *mut _);
            // SAFETY: el evento vive durante todo el proceso (no se cierra).
            while unsafe { WaitForSingleObject(ev, INFINITE) } == WAIT_OBJECT_0 {
                on_show_request();
            }
        })
        .map_err(|e| crate::PlatformError::Invalid(e.to_string()))?;
    Ok(Instance::Primary(PrimaryGuard { mutex }))
}
