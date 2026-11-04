//! Descubrimiento de aplicaciones: accesos directos del menú Inicio, apps UWP/MSIX y
//! ejecutables del `PATH` (RF-030).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, KF_FLAG_DEFAULT,
    SHGetKnownFolderItem, SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
};

/// Tipo de aplicación y cómo lanzarla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppKind {
    /// Acceso directo `.lnk` del menú Inicio.
    Shortcut {
        /// Ruta del `.lnk`.
        lnk: PathBuf,
        /// Ejecutable de destino, si se pudo resolver.
        target: Option<PathBuf>,
    },
    /// App empaquetada (UWP/MSIX), identificada por su AUMID.
    Packaged {
        /// `PackageFamilyName!AppId`.
        aumid: String,
    },
    /// Ejecutable encontrado en el `PATH`.
    Exe {
        /// Ruta del ejecutable.
        path: PathBuf,
    },
}

/// Una aplicación descubierta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    /// Nombre visible.
    pub name: String,
    /// Tipo y destino.
    pub kind: AppKind,
}

impl AppKind {
    /// Texto para `ShellExecuteExW`.
    pub fn launch_target(&self) -> String {
        match self {
            Self::Shortcut { lnk, .. } => lnk.to_string_lossy().into_owned(),
            Self::Packaged { aumid } => format!("shell:AppsFolder\\{aumid}"),
            Self::Exe { path } => path.to_string_lossy().into_owned(),
        }
    }

    /// Ruta en disco que representa la app (para «abrir ubicación» y «copiar ruta»).
    pub fn file_path(&self) -> Option<&Path> {
        match self {
            Self::Shortcut {
                target: Some(t), ..
            } => Some(t),
            Self::Shortcut { lnk, .. } => Some(lnk),
            Self::Packaged { .. } => None,
            Self::Exe { path } => Some(path),
        }
    }

    /// Nombre para el sistema de iconos del shell.
    pub fn icon_source(&self) -> String {
        self.launch_target()
    }
}

fn start_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["ProgramData", "APPDATA"] {
        if let Some(base) = std::env::var_os(var) {
            dirs.push(
                PathBuf::from(base)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs"),
            );
        }
    }
    dirs
}

fn is_uninstaller(name: &str, target: Option<&Path>) -> bool {
    let n = name.to_lowercase();
    if n.contains("uninstall") || n.contains("desinstalar") {
        return true;
    }
    target
        .and_then(|t| t.file_name())
        .map(|f| f.to_string_lossy().to_lowercase())
        .is_some_and(|f| f.starts_with("unins") || f.starts_with("uninstall"))
}

fn walk_lnk(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 6 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => walk_lnk(&p, out, depth + 1),
            Ok(_) if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) => out.push(p),
            _ => {}
        }
    }
}

/// Accesos directos del menú Inicio (común y del usuario), sin desinstaladores.
pub fn start_menu_shortcuts() -> Vec<AppEntry> {
    let mut files = Vec::new();
    for d in start_menu_dirs() {
        walk_lnk(&d, &mut files, 0);
    }
    files
        .into_iter()
        .filter_map(|lnk| {
            let name = lnk.file_stem()?.to_string_lossy().into_owned();
            let target = crate::shell::resolve_shortcut(&lnk);
            if is_uninstaller(&name, target.as_deref()) {
                return None;
            }
            Some(AppEntry {
                name,
                kind: AppKind::Shortcut { lnk, target },
            })
        })
        .collect()
}

fn display_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
    // SAFETY: GetDisplayName devuelve un PWSTR reservado con CoTaskMemAlloc que se libera aquí.
    unsafe {
        let p = item.GetDisplayName(kind).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0.cast()));
        s
    }
}

/// Apps empaquetadas (UWP/MSIX) de la carpeta virtual «Aplicaciones».
pub fn packaged_apps() -> Vec<AppEntry> {
    crate::com::ensure_thread();
    let mut out = Vec::new();
    // SAFETY: llamadas COM en un hilo con COM inicializado; las interfaces se liberan al salir
    // de ámbito.
    unsafe {
        let Ok(folder) =
            SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)
        else {
            return out;
        };
        let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else {
            return out;
        };
        loop {
            let mut batch = [None];
            let mut fetched = 0u32;
            if items.Next(&mut batch, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = batch[0].take() else { break };
            let (Some(name), Some(id)) = (
                display_name(&item, SIGDN_NORMALDISPLAY),
                display_name(&item, SIGDN_PARENTRELATIVEPARSING),
            ) else {
                continue;
            };
            // Los AUMID de apps empaquetadas tienen la forma `Familia!App`; el resto ya llega
            // por los accesos directos.
            if id.contains('!') && !is_uninstaller(&name, None) {
                out.push(AppEntry {
                    name,
                    kind: AppKind::Packaged { aumid: id },
                });
            }
        }
    }
    out
}

/// Ejecutables (`.exe`) de las carpetas del `PATH`, sin duplicados por nombre.
pub fn path_executables() -> Vec<AppEntry> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in std::env::split_paths(&path) {
        // System32 y las utilidades POSIX de Git/MSYS (`usr\bin`) tienen cientos de ejecutables
        // sin interés como «app» que ensucian los resultados.
        let lower = dir.to_string_lossy().to_lowercase();
        let lower = lower.trim_end_matches('\\');
        if lower.contains("\\windows\\system32") || lower.ends_with("\\usr\\bin") {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
                continue;
            }
            let Some(stem) = p.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            if seen.insert(stem.to_lowercase()) {
                out.push(AppEntry {
                    name: stem,
                    kind: AppKind::Exe { path: p },
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_uninstallers() {
        assert!(is_uninstaller("Uninstall Foo", None));
        assert!(is_uninstaller(
            "Foo",
            Some(Path::new("C:\\Program Files\\Foo\\unins000.exe"))
        ));
        assert!(!is_uninstaller("Foo", Some(Path::new("C:\\Foo\\foo.exe"))));
    }

    #[test]
    fn finds_some_apps_on_this_machine() {
        // Prueba de humo: cualquier Windows tiene accesos directos o apps empaquetadas.
        let n = start_menu_shortcuts().len() + packaged_apps().len();
        assert!(n > 0);
    }
}
