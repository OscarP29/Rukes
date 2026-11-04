//! Inicialización de COM por hilo.

use std::cell::Cell;

use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx,
};

thread_local! {
    static INIT: Cell<bool> = const { Cell::new(false) };
}

/// Inicializa COM (STA) en el hilo actual si no se hizo ya. Necesario antes de usar el shell.
pub fn ensure_thread() {
    INIT.with(|i| {
        if !i.get() {
            // SAFETY: CoInitializeEx no tiene precondiciones; un fallo (p. ej. ya inicializado en
            // otro modo) deja COM utilizable y se ignora.
            let _ =
                unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
            i.set(true);
        }
    });
}
