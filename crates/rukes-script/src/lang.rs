//! Lenguajes e intérpretes (RF-131): de un script a un programa con sus argumentos.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Lenguaje de un script según su extensión.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lang {
    /// `.ps1`
    PowerShell,
    /// `.bat` / `.cmd`
    Batch,
    /// `.exe`
    Exe,
    /// `.rhai`, embebido.
    Rhai,
    /// Cualquier otra extensión con un intérprete configurado (`.py`, `.js`, `.lua`...).
    Interpreted(String),
}

impl Lang {
    /// Lenguaje de un archivo, si Rukes sabe ejecutarlo con esta configuración.
    pub fn of(path: &Path, interpreters: &BTreeMap<String, String>) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "ps1" => Self::PowerShell,
            "bat" | "cmd" => Self::Batch,
            "exe" => Self::Exe,
            "rhai" => Self::Rhai,
            e if interpreters.contains_key(e) => Self::Interpreted(e.to_owned()),
            _ => return None,
        })
    }
}

/// Programa a lanzar y argumentos que van antes de los del usuario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    /// Ejecutable.
    pub program: PathBuf,
    /// Argumentos fijos (intérprete, ruta del script).
    pub args: Vec<OsString>,
}

/// Separa un comando en palabras (admite `"comillas"`).
pub fn split_command(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Busca un programa como lo haría la consola: ruta directa o en el `PATH`, probando las
/// extensiones de `PATHEXT` si no lleva una.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|e| !e.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let candidates = |base: &Path| -> Option<PathBuf> {
        if base.extension().is_some() && base.is_file() {
            return Some(base.to_path_buf());
        }
        exts.iter().find_map(|e| {
            let mut p = base.as_os_str().to_owned();
            p.push(e);
            let p = PathBuf::from(p);
            p.is_file().then_some(p)
        })
    };
    let direct = crate::args::expand_path(name);
    if direct.is_absolute() || name.contains(['/', '\\']) {
        return candidates(&direct);
    }
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| candidates(&dir.join(name)))
}

/// PowerShell 7 si está instalado; si no, Windows PowerShell.
pub fn powershell() -> PathBuf {
    find_in_path("pwsh").unwrap_or_else(|| PathBuf::from("powershell.exe"))
}

/// Intérpretes que se prueban si el configurado no existe.
fn fallbacks(ext: &str) -> &'static [&'static str] {
    match ext {
        "js" | "mjs" => &["node", "deno run", "bun run"],
        "ts" => &["deno run", "bun run"],
        "py" | "pyw" => &["python", "py", "python3"],
        _ => &[],
    }
}

/// Programa y argumentos para ejecutar `script`. `interpreter` es el de la cabecera
/// (`@rukes.interpreter`), que manda sobre el de la config.
pub fn launch(
    script: &Path,
    lang: &Lang,
    interpreter: Option<&str>,
    interpreters: &BTreeMap<String, String>,
    interactive: bool,
) -> Result<Launch, String> {
    let path = script.as_os_str().to_owned();
    let with_interpreter = |cmd: &str| -> Option<Launch> {
        let mut words = split_command(cmd);
        if words.is_empty() {
            return None;
        }
        let program = find_in_path(&words.remove(0))?;
        let mut args: Vec<OsString> = words.into_iter().map(OsString::from).collect();
        args.push(path.clone());
        Some(Launch { program, args })
    };
    if let Some(cmd) = interpreter {
        return with_interpreter(cmd)
            .ok_or_else(|| format!("no se encontró el intérprete «{cmd}»"));
    }
    Ok(match lang {
        Lang::PowerShell => {
            let mut args: Vec<OsString> = vec!["-NoLogo".into(), "-NoProfile".into()];
            if !interactive {
                args.push("-NonInteractive".into());
            }
            args.extend(["-ExecutionPolicy".into(), "Bypass".into(), "-File".into()]);
            args.push(path);
            Launch {
                program: powershell(),
                args,
            }
        }
        // La biblioteca estándar lanza los .bat/.cmd a través de cmd.exe escapando los
        // argumentos (y se niega si no puede hacerlo de forma segura).
        Lang::Batch | Lang::Exe => Launch {
            program: script.to_path_buf(),
            args: Vec::new(),
        },
        Lang::Rhai => return Err("los scripts .rhai se ejecutan dentro de Rukes".into()),
        Lang::Interpreted(ext) => {
            let configured = interpreters.get(ext).map(String::as_str);
            let mut tried = Vec::new();
            for cmd in configured.into_iter().chain(fallbacks(ext).iter().copied()) {
                if tried.contains(&cmd) {
                    continue;
                }
                tried.push(cmd);
                if let Some(l) = with_interpreter(cmd) {
                    return Ok(l);
                }
            }
            return Err(format!(
                "no se encontró un intérprete para .{ext} (probado: {}); configúralo en [scripts.interpreters]",
                tried.join(", ")
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interpreters() -> BTreeMap<String, String> {
        [("py", "python"), ("js", "node"), ("foo", "cmd /d /c")]
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect()
    }

    #[test]
    fn languages_by_extension() {
        let i = interpreters();
        assert_eq!(Lang::of(Path::new("a.PS1"), &i), Some(Lang::PowerShell));
        assert_eq!(Lang::of(Path::new("a.cmd"), &i), Some(Lang::Batch));
        assert_eq!(Lang::of(Path::new("a.rhai"), &i), Some(Lang::Rhai));
        assert_eq!(
            Lang::of(Path::new("a.py"), &i),
            Some(Lang::Interpreted("py".into()))
        );
        assert_eq!(Lang::of(Path::new("a.txt"), &i), None);
        assert_eq!(Lang::of(Path::new("README"), &i), None);
    }

    #[test]
    fn split_and_find() {
        assert_eq!(split_command("deno run -A"), ["deno", "run", "-A"]);
        assert_eq!(
            split_command("\"C:\\Program Files\\x.exe\" --a \"\""),
            ["C:\\Program Files\\x.exe", "--a", ""]
        );
        let cmd = find_in_path("cmd").unwrap();
        assert!(cmd.to_string_lossy().to_lowercase().ends_with("cmd.exe"));
        assert!(find_in_path("no-existe-rukes-xyz").is_none());
        assert!(find_in_path("C:\\Windows\\System32\\cmd.exe").is_some());
    }

    #[test]
    fn launch_lines() {
        let i = interpreters();
        let ps = launch(
            Path::new("C:\\s\\a.ps1"),
            &Lang::PowerShell,
            None,
            &i,
            false,
        )
        .unwrap();
        assert!(ps.args.contains(&OsString::from("-NonInteractive")));
        assert_eq!(ps.args.last(), Some(&OsString::from("C:\\s\\a.ps1")));
        let bat = launch(Path::new("C:\\s\\a.cmd"), &Lang::Batch, None, &i, false).unwrap();
        assert_eq!(bat.program, PathBuf::from("C:\\s\\a.cmd"));
        let foo = launch(
            Path::new("C:\\s\\a.foo"),
            &Lang::Interpreted("foo".into()),
            None,
            &i,
            false,
        )
        .unwrap();
        assert!(
            foo.program
                .to_string_lossy()
                .to_lowercase()
                .ends_with("cmd.exe")
        );
        assert_eq!(foo.args, ["/d", "/c", "C:\\s\\a.foo"].map(OsString::from));
        let none = launch(
            Path::new("a.zzz"),
            &Lang::Interpreted("zzz".into()),
            None,
            &BTreeMap::from([("zzz".to_owned(), "no-existe-rukes".to_owned())]),
            false,
        );
        assert!(none.unwrap_err().contains("no-existe-rukes"));
        let custom = launch(
            Path::new("a.py"),
            &Lang::Interpreted("py".into()),
            Some("cmd /c"),
            &i,
            false,
        )
        .unwrap();
        assert_eq!(custom.args, ["/c", "a.py"].map(OsString::from));
    }
}
