//! Controles del sistema (RF-063): volumen, brillo, modo claro/oscuro y radios (Wi-Fi,
//! Bluetooth, modo avión).

use std::time::Duration;

use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    CoSetProxyBlanket, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_DWORD, RegSetKeyValueW};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE, WbemLocator,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use windows::core::{BSTR, PCWSTR};

use crate::{Context as _, PlatformError, Result, wide};

const RPC_C_AUTHN_WINNT: u32 = 10;
const RPC_C_AUTHZ_NONE: u32 = 0;

// ---------------------------------------------------------------- volumen

fn endpoint() -> Result<IAudioEndpointVolume> {
    crate::com::ensure_thread();
    // SAFETY: objetos COM de audio en un hilo con COM inicializado.
    unsafe {
        let e: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ctx("MMDeviceEnumerator")?;
        let dev = e
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .ctx("GetDefaultAudioEndpoint")?;
        dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            .ctx("IAudioEndpointVolume")
    }
}

/// Volumen de la salida predeterminada (0..=100) y si está silenciada.
pub fn volume() -> Result<(u32, bool)> {
    let ep = endpoint()?;
    // SAFETY: métodos de lectura sobre un objeto válido.
    unsafe {
        let v = ep.GetMasterVolumeLevelScalar().ctx("GetMasterVolume")?;
        let m = ep.GetMute().ctx("GetMute")?.as_bool();
        Ok(((v * 100.0).round() as u32, m))
    }
}

/// Fija el volumen (0..=100); quita el silencio.
pub fn set_volume(percent: u32) -> Result<()> {
    let ep = endpoint()?;
    let v = percent.min(100) as f32 / 100.0;
    // SAFETY: métodos sobre un objeto válido; sin contexto de evento.
    unsafe {
        ep.SetMasterVolumeLevelScalar(v, std::ptr::null())
            .ctx("SetMasterVolume")?;
        ep.SetMute(false, std::ptr::null()).ctx("SetMute")
    }
}

/// Silencia o no la salida predeterminada.
pub fn set_mute(mute: bool) -> Result<()> {
    let ep = endpoint()?;
    // SAFETY: método sobre un objeto válido; sin contexto de evento.
    unsafe { ep.SetMute(mute, std::ptr::null()).ctx("SetMute") }
}

// ---------------------------------------------------------------- brillo (WMI)

fn wmi() -> Result<IWbemServices> {
    crate::com::ensure_thread();
    // SAFETY: conexión WMI local estándar; el proxy se configura para suplantar al usuario.
    unsafe {
        let loc: IWbemLocator =
            CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ctx("WbemLocator")?;
        let svc = loc
            .ConnectServer(
                &BSTR::from("root\\WMI"),
                &BSTR::new(),
                &BSTR::new(),
                &BSTR::new(),
                0,
                &BSTR::new(),
                None,
            )
            .ctx("ConnectServer")?;
        CoSetProxyBlanket(
            &svc,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
        .ctx("CoSetProxyBlanket")?;
        Ok(svc)
    }
}

fn first(svc: &IWbemServices, query: &str) -> Result<Option<IWbemClassObject>> {
    // SAFETY: consulta WQL sobre un servicio válido; el enumerador devuelve objetos propios.
    unsafe {
        let en = svc
            .ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(query),
                WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0),
                None,
            )
            .ctx("ExecQuery")?;
        let mut objs = [None];
        let mut n = 0u32;
        let _ = en.Next(WBEM_INFINITE, &mut objs, &mut n);
        Ok(if n == 0 { None } else { objs[0].take() })
    }
}

fn get(obj: &IWbemClassObject, name: &str) -> Result<VARIANT> {
    let n = wide(name);
    let mut v = VARIANT::default();
    // SAFETY: `n` vive durante la llamada; `v` recibe una copia propia.
    unsafe {
        obj.Get(PCWSTR(n.as_ptr()), 0, &mut v, None, None)
            .ctx("IWbemClassObject::Get")?;
    }
    Ok(v)
}

/// Brillo de la pantalla integrada (0..=100). `None` si no se puede controlar (monitor externo
/// o equipo de sobremesa).
pub fn brightness() -> Option<u32> {
    let svc = wmi().ok()?;
    let obj = first(&svc, "SELECT CurrentBrightness FROM WmiMonitorBrightness").ok()??;
    let v = get(&obj, "CurrentBrightness").ok()?;
    u32::try_from(&v).ok().map(|b| b.min(100))
}

/// Fija el brillo de la pantalla integrada (0..=100).
pub fn set_brightness(percent: u32) -> Result<()> {
    let svc = wmi()?;
    let inst = first(&svc, "SELECT * FROM WmiMonitorBrightnessMethods")?
        .ok_or_else(|| PlatformError::Invalid("brillo no disponible en esta pantalla".into()))?;
    let path = get(&inst, "__PATH")?;
    let path = BSTR::try_from(&path).ctx("__PATH")?;
    // SAFETY: se obtiene la firma del método, se rellena una instancia de parámetros y se llama
    // sobre la ruta de la instancia; todos los objetos son propios.
    unsafe {
        let mut class = None;
        svc.GetObject(
            &BSTR::from("WmiMonitorBrightnessMethods"),
            WBEM_GENERIC_FLAG_TYPE(0),
            None,
            Some(&mut class),
            None,
        )
        .ctx("GetObject")?;
        let class = class.ok_or_else(|| PlatformError::Invalid("WMI".into()))?;
        let mut sig = None;
        let name = wide("WmiSetBrightness");
        class
            .GetMethod(PCWSTR(name.as_ptr()), 0, &mut sig, std::ptr::null_mut())
            .ctx("GetMethod")?;
        let sig = sig.ok_or_else(|| PlatformError::Invalid("WMI".into()))?;
        let params = sig.SpawnInstance(0).ctx("SpawnInstance")?;
        let (t, b) = (wide("Timeout"), wide("Brightness"));
        params
            .Put(PCWSTR(t.as_ptr()), 0, &VARIANT::from(1u32), 0)
            .ctx("Put Timeout")?;
        params
            .Put(
                PCWSTR(b.as_ptr()),
                0,
                &VARIANT::from(percent.min(100) as u8),
                0,
            )
            .ctx("Put Brightness")?;
        svc.ExecMethod(
            &path,
            &BSTR::from("WmiSetBrightness"),
            WBEM_GENERIC_FLAG_TYPE(0),
            None,
            &params,
            None,
            None,
        )
        .ctx("ExecMethod")
    }
}

// ---------------------------------------------------------------- modo claro/oscuro

/// Cambia el modo de Windows y de las apps a oscuro (`true`) o claro.
pub fn set_dark_mode(dark: bool) -> Result<()> {
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value: u32 = u32::from(!dark);
    for name in ["AppsUseLightTheme", "SystemUsesLightTheme"] {
        let n = wide(name);
        // SAFETY: cadenas terminadas en cero que viven durante la llamada; el dato es un DWORD.
        let r = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(n.as_ptr()),
                REG_DWORD.0,
                Some((&raw const value).cast()),
                4,
            )
        };
        r.ok().ctx("RegSetKeyValueW")?;
    }
    let area = wide("ImmersiveColorSet");
    // SAFETY: aviso a todas las ventanas de que cambió el tema; `area` vive durante la llamada y
    // se espera como mucho 200 ms por ventana.
    unsafe {
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(area.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- radios

/// Tipo de radio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioType {
    /// Wi-Fi.
    WiFi,
    /// Bluetooth.
    Bluetooth,
    /// Datos móviles.
    Mobile,
    /// Otra.
    Other,
}

/// Una radio del equipo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioInfo {
    /// Nombre.
    pub name: String,
    /// Tipo.
    pub kind: RadioType,
    /// Encendida.
    pub on: bool,
}

/// Ejecuta `f` en un hilo MTA propio (la API de radios es asíncrona de WinRT) con tiempo máximo.
fn in_mta<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    std::thread::Builder::new()
        .name("rukes-radios".into())
        .spawn(move || {
            // SAFETY: inicializa COM en este hilo nuevo, que solo hace esta tarea.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let _ = tx.send(f());
        })
        .map_err(|e| PlatformError::Invalid(e.to_string()))?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|_| PlatformError::Invalid("las radios no respondieron".into()))?
}

fn radios_raw() -> Result<Vec<Radio>> {
    let access = Radio::RequestAccessAsync()
        .and_then(|op| op.join())
        .ctx("RequestAccessAsync")?;
    if access != RadioAccessStatus::Allowed {
        return Err(PlatformError::Invalid(
            "Windows no permite controlar las radios".into(),
        ));
    }
    let list = Radio::GetRadiosAsync()
        .and_then(|op| op.join())
        .ctx("GetRadiosAsync")?;
    Ok(list.into_iter().collect())
}

fn kind_of(k: RadioKind) -> RadioType {
    match k {
        RadioKind::WiFi => RadioType::WiFi,
        RadioKind::Bluetooth => RadioType::Bluetooth,
        RadioKind::MobileBroadband => RadioType::Mobile,
        _ => RadioType::Other,
    }
}

/// Radios del equipo y su estado.
pub fn radios() -> Result<Vec<RadioInfo>> {
    in_mta(|| {
        Ok(radios_raw()?
            .iter()
            .filter_map(|r| {
                Some(RadioInfo {
                    name: r.Name().ok()?.to_string(),
                    kind: kind_of(r.Kind().ok()?),
                    on: r.State().ok()? == RadioState::On,
                })
            })
            .collect())
    })
}

/// Enciende o apaga las radios de un tipo (`None` = todas: modo avión). Devuelve cuántas cambió.
pub fn set_radios(kind: Option<RadioType>, on: bool) -> Result<usize> {
    in_mta(move || {
        let state = if on { RadioState::On } else { RadioState::Off };
        let mut n = 0;
        for r in radios_raw()? {
            let k = r.Kind().map(kind_of).unwrap_or(RadioType::Other);
            if kind.is_some_and(|want| want != k) || k == RadioType::Other {
                continue;
            }
            let status = r
                .SetStateAsync(state)
                .and_then(|op| op.join())
                .ctx("SetStateAsync")?;
            if status == RadioAccessStatus::Allowed {
                n += 1;
            }
        }
        Ok(n)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Solo lectura: no cambia nada del equipo.
    #[test]
    fn reads_volume() {
        // Puede no haber salida de audio (servidores de CI): basta con que no falle de forma rara.
        if let Ok((v, _)) = volume() {
            assert!(v <= 100);
        }
        if let Some(b) = brightness() {
            assert!(b <= 100);
        }
    }
}
