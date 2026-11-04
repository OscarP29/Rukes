//! Hora local y en otras zonas horarias con la base de datos de Windows (RF-073).

use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
use windows::Win32::System::Time::{
    DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation,
    SystemTimeToTzSpecificLocalTimeEx,
};

/// Fecha y hora civil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    /// Año.
    pub year: i32,
    /// Mes (1-12).
    pub month: u32,
    /// Día (1-31).
    pub day: u32,
    /// Hora (0-23).
    pub hour: u32,
    /// Minuto.
    pub minute: u32,
    /// Segundo.
    pub second: u32,
}

impl From<SYSTEMTIME> for DateTime {
    fn from(t: SYSTEMTIME) -> Self {
        Self {
            year: i32::from(t.wYear),
            month: u32::from(t.wMonth),
            day: u32::from(t.wDay),
            hour: u32::from(t.wHour),
            minute: u32::from(t.wMinute),
            second: u32::from(t.wSecond),
        }
    }
}

/// Hora local.
pub fn local_now() -> DateTime {
    // SAFETY: sin precondiciones.
    unsafe { GetLocalTime() }.into()
}

fn key_name(tz: &DYNAMIC_TIME_ZONE_INFORMATION) -> String {
    let k = &tz.TimeZoneKeyName;
    let len = k.iter().position(|&c| c == 0).unwrap_or(k.len());
    String::from_utf16_lossy(&k[..len])
}

/// Hora actual en la zona de Windows indicada (p. ej. `Tokyo Standard Time`).
pub fn now_in_zone(zone_key: &str) -> Option<DateTime> {
    let mut i = 0u32;
    loop {
        let mut tz = DYNAMIC_TIME_ZONE_INFORMATION::default();
        // SAFETY: estructura local; el índice fuera de rango devuelve un código de error.
        if unsafe { EnumDynamicTimeZoneInformation(i, &mut tz) } != 0 {
            return None;
        }
        if key_name(&tz).eq_ignore_ascii_case(zone_key) {
            let mut local = SYSTEMTIME::default();
            // SAFETY: estructuras locales válidas.
            unsafe {
                let utc = GetSystemTime();
                SystemTimeToTzSpecificLocalTimeEx(Some(&tz), &utc, &mut local).ok()?;
            }
            return Some(local.into());
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_zone_resolves_and_unknown_does_not() {
        assert!(now_in_zone("Tokyo Standard Time").is_some());
        assert!(now_in_zone("UTC").is_some());
        assert!(now_in_zone("Nowhere Standard Time").is_none());
    }
}
