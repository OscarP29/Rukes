//! Comandos de shell (`>`), URIs `shell:`/`ms-settings:` y comandos del sistema
//! (RF-060, RF-061, RF-062).

use std::path::PathBuf;

use base64::Engine as _;
use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata,
    Query, ResultItem,
};
use rukes_platform::power::{self, Command};
use rukes_platform::shell::{self, Verb};

/// Ajustes del plugin de shell.
#[derive(Debug, Clone)]
pub struct ShellSettings {
    /// Prefijo (`>`).
    pub prefix: String,
    /// `pwsh`, `powershell`, `cmd` o `nu`.
    pub shell: String,
    /// Terminal: `wt` (Windows Terminal) o vacío para la consola del propio shell.
    pub terminal: String,
    /// No cerrar la terminal al terminar.
    pub keep_open: bool,
}

/// Plugin de shell.
pub struct ShellPlugin {
    /// Ajustes.
    pub settings: ShellSettings,
    /// Carpeta de trabajo de los comandos.
    pub cwd: Option<PathBuf>,
}

/// `true` si el texto es una URI que se abre con el shell (`shell:startup`, `ms-settings:`...).
/// Una letra de unidad (`C:\`) no cuenta.
pub fn is_uri(s: &str) -> bool {
    let Some((scheme, _)) = s.split_once(':') else {
        return false;
    };
    scheme.len() >= 2
        && !s.contains(char::is_whitespace)
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+' || c == '.')
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
}

/// Programa y argumentos para ejecutar `cmd` según los ajustes.
pub fn command_line(s: &ShellSettings, cmd: &str) -> (String, String) {
    let (exe, args) = match s.shell.as_str() {
        "cmd" => (
            "cmd.exe".to_owned(),
            format!("{} {cmd}", if s.keep_open { "/K" } else { "/C" }),
        ),
        "nu" | "nushell" => (
            "nu.exe".to_owned(),
            format!(
                "{} \"{}\"",
                if s.keep_open { "-e" } else { "-c" },
                cmd.replace('"', "\\\"")
            ),
        ),
        other => {
            // PowerShell: -EncodedCommand evita cualquier problema de comillas.
            let exe = if other == "powershell" {
                "powershell.exe"
            } else {
                "pwsh.exe"
            };
            let utf16: Vec<u8> = cmd.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let b64 = base64::engine::general_purpose::STANDARD.encode(utf16);
            let no_exit = if s.keep_open { "-NoExit " } else { "" };
            (
                exe.to_owned(),
                format!("-NoLogo {no_exit}-EncodedCommand {b64}"),
            )
        }
    };
    if s.terminal == "wt" {
        // En wt, `;` separa subcomandos.
        (
            "wt.exe".to_owned(),
            format!("{exe} {}", args.replace(';', "\\;")),
        )
    } else {
        (exe, args)
    }
}

impl ShellPlugin {
    fn run(&self, cmd: &str, verb: Verb) -> Result<(), PluginError> {
        let (exe, args) = command_line(&self.settings, cmd);
        let cwd = self.cwd.as_deref();
        shell::launch(&exe, Some(&args), cwd, verb)
            .or_else(|e| {
                // Sin Windows Terminal (o sin pwsh), se prueba la consola del shell / PowerShell 5.
                let mut fallback = self.settings.clone();
                fallback.terminal.clear();
                if self.settings.terminal.is_empty() && fallback.shell == "pwsh" {
                    fallback.shell = "powershell".into();
                } else if !self.settings.terminal.is_empty() {
                    // Mismo shell, sin terminal.
                } else {
                    return Err(e);
                }
                let (exe, args) = command_line(&fallback, cmd);
                shell::launch(&exe, Some(&args), cwd, verb)
            })
            .map_err(|e| PluginError::Failed(e.to_string()))
    }
}

impl Plugin for ShellPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "shell".into(),
            name: "Shell".into(),
            prefix: Some(self.settings.prefix.clone()),
            keywords: Vec::new(),
            priority: 0.5,
            global: false,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let cmd = q.text.trim();
        if cmd.is_empty() {
            sink(Vec::new());
            return;
        }
        let mut it = if is_uri(cmd) {
            let mut it = ResultItem::new("uri", trf("shell.open", &[("cmd", &cmd)]));
            it.subtitle = tr("shell.uri").into();
            it.payload = format!("uri:{cmd}");
            it
        } else {
            let mut it = ResultItem::new("run", trf("shell.run", &[("cmd", &cmd)]));
            let admin = tr("shell.admin_hint");
            it.subtitle = format!("{} · {admin}", self.settings.shell);
            it.payload = format!("cmd:{cmd}");
            it
        };
        it.actions
            .push(Action::new(ActionId::RUN_AS_ADMIN, "action.run_as_admin"));
        it.icon = Icon::Named("shell".into());
        it.fuzzy = 1.0;
        sink(vec![it]);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let verb = match action {
            ActionId::OPEN => Verb::Open,
            ActionId::RUN_AS_ADMIN => Verb::RunAs,
            other => return Err(PluginError::UnsupportedAction(other.into())),
        };
        if let Some(uri) = item.payload.strip_prefix("uri:") {
            shell::launch(uri, None, None, verb).map_err(|e| PluginError::Failed(e.to_string()))?;
        } else if let Some(cmd) = item.payload.strip_prefix("cmd:") {
            self.run(cmd, verb)?;
        }
        Ok(ActionOutcome::Hide)
    }
}

// ---------------------------------------------------------------------------------------------

/// Plugin de comandos del sistema (bloquear, apagar...).
pub struct SystemCommandsPlugin {
    /// Pedir confirmación antes de los comandos que cierran la sesión o pierden datos.
    pub confirm: bool,
}

struct Entry {
    id: &'static str,
    cmd: Command,
    key: &'static str,
    words: &'static str,
}

const ENTRIES: &[Entry] = &[
    Entry {
        id: "lock",
        cmd: Command::Lock,
        key: "syscmd.lock",
        words: "lock bloquear",
    },
    Entry {
        id: "sleep",
        cmd: Command::Sleep,
        key: "syscmd.sleep",
        words: "sleep suspender dormir",
    },
    Entry {
        id: "hibernate",
        cmd: Command::Hibernate,
        key: "syscmd.hibernate",
        words: "hibernate hibernar",
    },
    Entry {
        id: "shutdown",
        cmd: Command::Shutdown,
        key: "syscmd.shutdown",
        words: "shutdown apagar poweroff",
    },
    Entry {
        id: "restart",
        cmd: Command::Restart,
        key: "syscmd.restart",
        words: "restart reboot reiniciar",
    },
    Entry {
        id: "logoff",
        cmd: Command::LogOff,
        key: "syscmd.logoff",
        words: "logoff logout signout cerrar sesion",
    },
    Entry {
        id: "emptybin",
        cmd: Command::EmptyRecycleBin,
        key: "syscmd.emptybin",
        words: "papelera recycle bin trash vaciar empty",
    },
];

fn entry(id: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.id == id)
}

fn title(e: &Entry) -> &'static str {
    tr(e.key)
}

impl SystemCommandsPlugin {
    fn needs_confirm(&self, e: &Entry) -> bool {
        self.confirm && e.cmd != Command::Lock
    }

    fn confirmation(e: &Entry) -> Vec<ResultItem> {
        let mut yes = ResultItem::new(
            format!("confirm:{}", e.id),
            trf("syscmd.confirm", &[("what", &title(e).to_lowercase())]),
        );
        yes.subtitle = tr("syscmd.confirm_sub").into();
        yes.icon = Icon::Named("system".into());
        let mut no = ResultItem::new("cancel", tr("common.cancel"));
        no.icon = Icon::Named("system".into());
        vec![yes, no]
    }
}

impl Plugin for SystemCommandsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "syscmd".into(),
            name: tr("syscmd.name").into(),
            prefix: None,
            keywords: Vec::new(),
            priority: 0.0,
            global: true,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let mut f = Fuzzy::new(&q.text);
        let items = ENTRIES
            .iter()
            .filter_map(|e| {
                let t = title(e);
                // Las palabras extra solo cuentan si coinciden de verdad (umbral alto).
                let extra = e
                    .words
                    .split(' ')
                    .filter_map(|w| f.score(w))
                    .fold(0.0f32, f32::max);
                let score = f
                    .score(t)
                    .unwrap_or(0.0)
                    .max(if extra >= 0.9 { extra } else { 0.0 });
                (score > 0.0).then(|| {
                    let mut it = ResultItem::new(e.id, t);
                    it.subtitle = tr("syscmd.subtitle").into();
                    it.icon = Icon::Named("system".into());
                    it.fuzzy = score;
                    it
                })
            })
            .collect();
        sink(items);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if action != ActionId::OPEN {
            return Err(PluginError::UnsupportedAction(action.into()));
        }
        if item.id == "cancel" {
            return Ok(ActionOutcome::ReplaceQuery(String::new()));
        }
        let (confirmed, id) = match item.id.strip_prefix("confirm:") {
            Some(id) => (true, id),
            None => (false, item.id.as_str()),
        };
        let e =
            entry(id).ok_or_else(|| PluginError::Failed(format!("comando desconocido: {id}")))?;
        if !confirmed && self.needs_confirm(e) {
            return Ok(ActionOutcome::ShowResults(Self::confirmation(e)));
        }
        power::run(e.cmd).map_err(|err| PluginError::Failed(err.to_string()))?;
        Ok(ActionOutcome::Hide)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(shell: &str, terminal: &str, keep_open: bool) -> ShellSettings {
        ShellSettings {
            prefix: ">".into(),
            shell: shell.into(),
            terminal: terminal.into(),
            keep_open,
        }
    }

    #[test]
    fn uris() {
        assert!(is_uri("shell:startup"));
        assert!(is_uri("ms-settings:display"));
        assert!(!is_uri("C:\\Windows"));
        assert!(!is_uri("echo a:b c"));
        assert!(!is_uri("dir"));
    }

    #[test]
    fn command_lines() {
        let (exe, args) = command_line(&settings("cmd", "", true), "dir");
        assert_eq!((exe.as_str(), args.as_str()), ("cmd.exe", "/K dir"));
        let (exe, args) = command_line(&settings("pwsh", "wt", true), "ls; pwd");
        assert_eq!(exe, "wt.exe");
        assert!(args.starts_with("pwsh.exe -NoLogo -NoExit -EncodedCommand "));
        // El comando va codificado: el `;` no llega a wt.
        assert!(!args.contains(';'));
        let b64 = args.rsplit(' ').next().unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let utf16: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16(&utf16).unwrap(), "ls; pwd");
        let (_, args) = command_line(&settings("cmd", "wt", false), "a;b");
        assert_eq!(args, "cmd.exe /C a\\;b");
    }

    #[test]
    fn use_case_3_shell_uri_item() {
        let p = ShellPlugin {
            settings: settings("pwsh", "wt", true),
            cwd: None,
        };
        let q = Query {
            id: rukes_core::QueryId(1),
            raw: "> shell:startup".into(),
            text: "shell:startup".into(),
            targeted: true,
        };
        let mut got = Vec::new();
        p.query(&q, &CancelToken::new(), &mut |b| got.extend(b));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].payload, "uri:shell:startup");
    }

    #[test]
    fn system_commands_ask_for_confirmation() {
        let _ = rukes_core::i18n::set_language("es", "");
        let p = SystemCommandsPlugin { confirm: true };
        let q = Query {
            id: rukes_core::QueryId(1),
            raw: "apagar".into(),
            text: "apagar".into(),
            targeted: false,
        };
        let mut got = Vec::new();
        p.query(&q, &CancelToken::new(), &mut |b| got.extend(b));
        let first = got
            .iter()
            .max_by(|a, b| a.fuzzy.total_cmp(&b.fuzzy))
            .unwrap();
        assert_eq!(first.id, "shutdown");
        // Sin confirmar no apaga: devuelve la pregunta.
        match p.execute(first, ActionId::OPEN).unwrap() {
            ActionOutcome::ShowResults(items) => {
                assert_eq!(items[0].id, "confirm:shutdown");
                assert_eq!(items[1].id, "cancel");
            }
            other => panic!("se esperaba confirmación: {other:?}"),
        }
        let cancel = ResultItem::new("cancel", "Cancelar");
        assert!(matches!(
            p.execute(&cancel, ActionId::OPEN).unwrap(),
            ActionOutcome::ReplaceQuery(q) if q.is_empty()
        ));
    }
}
