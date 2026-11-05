//! Esquema de `config.toml`. Todo tiene valor por defecto: un archivo vacío es válido.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Configuración principal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Config {
    /// Ajustes generales.
    pub general: General,
    /// Ventana.
    pub window: Window,
    /// Búsqueda y ranking.
    pub search: Search,
    /// Plugins.
    pub plugins: Plugins,
    /// Scripts del usuario (sección 5.13).
    pub scripts: Scripts,
    /// Workflows y triggers (secciones 5.14 y 5.15).
    pub workflows: Workflows,
}

/// `[workflows]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Workflows {
    /// Activo.
    pub enabled: bool,
    /// Carpeta. Vacío = `workflows` dentro de la configuración. Admite `~` y `%VAR%`.
    pub dir: String,
    /// Ejecutar los triggers programados (RF-160). También se pausan desde la bandeja.
    pub triggers: bool,
}

impl Default for Workflows {
    fn default() -> Self {
        Self {
            enabled: true,
            dir: String::new(),
            triggers: true,
        }
    }
}

/// `[scripts]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Scripts {
    /// Activo.
    pub enabled: bool,
    /// Prioridad.
    pub priority: f32,
    /// Carpeta vigilada (RF-130). Vacío = `scripts` dentro de la carpeta de configuración.
    /// Admite `~` y variables `%VAR%`.
    pub dir: String,
    /// Editor para `new script` y «Editar» (RF-135): un comando del `PATH` o una ruta. Si no
    /// se encuentra, se usa el Bloc de notas.
    pub editor: String,
    /// Tiempo máximo por defecto (`30s`, `2m`, `0` = sin límite) (RF-137).
    pub default_timeout: String,
    /// Extensión → intérprete (RF-131). Sustituye a los valores por defecto de cada extensión.
    pub interpreters: BTreeMap<String, String>,
    /// Ejecuciones que se guardan en el historial (`logs`) (RF-138).
    pub history_size: usize,
}

impl Default for Scripts {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 0.05,
            dir: String::new(),
            editor: "code".into(),
            default_timeout: "30s".into(),
            interpreters: [
                ("py", "python"),
                ("js", "node"),
                ("ts", "deno run"),
                ("lua", "lua"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect(),
            history_size: 50,
        }
    }
}

/// `[general]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct General {
    /// Atajo global (RF-001).
    pub hotkey: String,
    /// Idioma de la interfaz: `auto`, `es`, `en`.
    pub language: String,
    /// Iniciar con Windows (RF-008).
    pub start_with_windows: bool,
    /// Terminal para los comandos de shell y «abrir terminal aquí»: `wt` (Windows Terminal) o
    /// `none` (la consola del propio shell).
    pub terminal: String,
    /// Tema (RF-192): uno incluido, uno de la carpeta `themes` o `system` (RF-198).
    pub theme: String,
    /// Tema cuando `theme = "system"` y el sistema está en modo claro.
    pub light_theme: String,
    /// Tema cuando `theme = "system"` y el sistema está en modo oscuro.
    pub dark_theme: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".into(),
            language: "auto".into(),
            start_with_windows: false,
            terminal: "wt".into(),
            theme: "system".into(),
            light_theme: "rukes-light".into(),
            dark_theme: "rukes-dark".into(),
        }
    }
}

/// Posición de la ventana (RF-002).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum WindowPosition {
    /// Centrada (un poco por encima del centro).
    #[default]
    Center,
    /// Centrada horizontalmente, cerca del borde superior.
    TopCenter,
    /// Centrada horizontalmente, cerca del borde inferior: la barra de búsqueda queda abajo y
    /// los resultados crecen hacia arriba.
    BottomCenter,
}

/// Qué hacer con la búsqueda anterior al mostrar la ventana (RF-004).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OnShow {
    /// Vaciar.
    Clear,
    /// Conservar.
    Keep,
    /// Conservar y seleccionar todo.
    #[default]
    SelectAll,
}

/// `[window]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Window {
    /// Posición.
    pub position: WindowPosition,
    /// Ancho lógico en píxeles.
    pub width: u32,
    /// Resultados visibles a la vez (RF-020).
    pub max_results: usize,
    /// Ocultar al perder el foco (RF-003).
    pub hide_on_blur: bool,
    /// Ocultar tras ejecutar una acción (RF-003).
    pub hide_after_action: bool,
    /// Comportamiento del texto al mostrar (RF-004).
    pub on_show: OnShow,
    /// Fondo: `auto` (el del tema), `mica`, `acrylic` o `solid` (RF-190).
    pub backdrop: String,
    /// Animaciones de 80–150 ms (RF-195). Se desactivan también si Windows las tiene apagadas.
    pub animations: bool,
    /// Panel de previsualización a la derecha (RF-017). Se alterna con Ctrl+P.
    pub preview_panel: bool,
    /// Ancho lógico del panel de previsualización.
    pub preview_width: u32,
    /// Con la búsqueda vacía, mostrar una leyenda con los comandos más útiles.
    pub hints: bool,
    /// Cuántos comandos muestra la leyenda.
    pub hints_count: usize,
    /// Qué comandos muestra la leyenda y en qué orden (`calc`, `files`, `new`...). Vacío: los de
    /// siempre.
    pub hints_items: Vec<String>,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            position: WindowPosition::default(),
            width: 680,
            max_results: 8,
            hide_on_blur: true,
            hide_after_action: true,
            on_show: OnShow::default(),
            backdrop: "auto".into(),
            animations: true,
            preview_panel: false,
            preview_width: 340,
            hints: true,
            hints_count: 6,
            hints_items: Vec::new(),
        }
    }
}

/// `[search]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Search {
    /// Peso de la frecency en el ranking (0..1).
    pub frecency_weight: f32,
    /// Vida media de la frecency en días.
    pub frecency_half_life_days: f32,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            frecency_weight: 0.35,
            frecency_half_life_days: 7.0,
        }
    }
}

/// `[plugins]`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Plugins {
    /// Plugin de aplicaciones.
    pub apps: Apps,
    /// Calculadora, unidades y fechas.
    pub calc: Calc,
    /// URLs y búsquedas web.
    pub web: Web,
    /// Comandos de shell y del sistema.
    pub shell: Shell,
    /// Ventanas abiertas.
    pub windows: PluginCommon,
    /// Procesos.
    pub processes: PluginCommon,
    /// Volumen, brillo, Wi-Fi, Bluetooth, modo avión y modo oscuro (RF-063).
    pub controls: PluginCommon,
    /// Peticiones HTTP (`http 3000/api`, `post :8080/login {...}`): un mini Postman.
    pub http: Http,
    /// Generadores, hashes y codificadores.
    pub generators: Generators,
    /// Archivos y carpetas.
    pub files: Files,
    /// Historial del portapapeles.
    pub clipboard: Clipboard,
    /// Snippets.
    pub snippets: Snippets,
    /// Notas, tareas y recordatorios.
    pub notes: Notes,
    /// Fondos de pantalla (RF-180).
    pub wall: Wall,
    /// Plugins externos (RF-170).
    pub external: External,
}

/// `[plugins.wall]` (RF-180)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Wall {
    /// Activo.
    pub enabled: bool,
    /// Palabra clave.
    pub keyword: String,
    /// Carpetas con imágenes (con subcarpetas). Admiten `~` y `%VAR%`.
    pub dirs: Vec<String>,
    /// Ajuste: `fill`, `fit`, `stretch`, `tile`, `center` o `span`.
    pub position: String,
    /// Cambiar de fondo cada cierto tiempo (`30m`, `2h`). Vacío = nunca.
    pub interval: String,
    /// Al rotar: `true` al azar, `false` en orden.
    pub shuffle: bool,
}

impl Default for Wall {
    fn default() -> Self {
        Self {
            enabled: true,
            keyword: "wall".into(),
            dirs: vec![
                "~/Pictures/Wallpapers".into(),
                "%SystemRoot%/Web/Wallpaper".into(),
            ],
            position: "fill".into(),
            interval: String::new(),
            shuffle: true,
        }
    }
}

/// `[plugins.external]` (RF-170..RF-173)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct External {
    /// Cargar plugins externos.
    pub enabled: bool,
    /// Carpeta (una subcarpeta con `plugin.toml` por plugin). Vacío = `plugins` dentro de la
    /// configuración.
    pub dir: String,
}

impl Default for External {
    fn default() -> Self {
        Self {
            enabled: true,
            dir: String::new(),
        }
    }
}

/// `[plugins.clipboard]` (sección 5.10)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Clipboard {
    /// Activo. Desactivado no se escucha el portapapeles ni se guarda nada (RF-104).
    pub enabled: bool,
    /// Palabra clave.
    pub keyword: String,
    /// Elementos guardados como mucho (los fijados no cuentan).
    pub max_items: usize,
    /// Días que se guarda cada elemento (0 = sin caducidad). Los fijados no caducan.
    pub expire_days: u32,
    /// `true`: Enter pega en la ventana anterior; `false`: solo copia.
    pub paste: bool,
    /// Ejecutables cuyas copias nunca se guardan (RF-102).
    pub exclude: Vec<String>,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self {
            enabled: true,
            keyword: "clip".into(),
            max_items: 500,
            expire_days: 30,
            paste: true,
            exclude: [
                "keepass.exe",
                "keepassxc.exe",
                "1password.exe",
                "bitwarden.exe",
                "enpass.exe",
                "dashlane.exe",
                "lastpass.exe",
                "nordpass.exe",
                "protonpass.exe",
                "roboform.exe",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

/// `[plugins.snippets]` (sección 5.11)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Snippets {
    /// Activo.
    pub enabled: bool,
    /// Palabra clave para listarlos todos.
    pub keyword: String,
    /// Carpeta. Vacío = `snippets` dentro de la configuración.
    pub dir: String,
    /// `true`: Enter inserta en la ventana anterior; `false`: solo copia.
    pub paste: bool,
}

impl Default for Snippets {
    fn default() -> Self {
        Self {
            enabled: true,
            keyword: "snip".into(),
            dir: String::new(),
            paste: true,
        }
    }
}

/// `[plugins.notes]` (sección 5.12)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Notes {
    /// Activo (notas, tareas y recordatorios).
    pub enabled: bool,
    /// Archivo de notas. Vacío = `notes/notas.md` dentro de la configuración.
    pub notes_file: String,
    /// Archivo de tareas. Vacío = `notes/tareas.md` dentro de la configuración.
    pub todo_file: String,
}

impl Default for Notes {
    fn default() -> Self {
        Self {
            enabled: true,
            notes_file: String::new(),
            todo_file: String::new(),
        }
    }
}

/// `[plugins.calc]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Calc {
    /// Activo.
    pub enabled: bool,
    /// Prefijo (RF-015).
    pub prefix: String,
    /// Evaluar también sin prefijo si la consulta parece una expresión.
    pub auto: bool,
    /// Conversión de divisas (`100 usd a eur`, RF-072).
    pub currency: bool,
    /// Actualizar las tasas en línea (open.er-api.com, como mucho dos veces al día). Desactivado
    /// por defecto: Rukes no se conecta a la red si no lo activas (sección 8). Sin esto se usan
    /// las de la caché.
    pub currency_online: bool,
    /// Conversor de colores (`#ff5733`, `rgb(...)`, `hsl(...)`, RF-075).
    pub colors: bool,
}

impl Default for Calc {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: "=".into(),
            auto: true,
            currency: true,
            currency_online: false,
            colors: true,
        }
    }
}

/// `[plugins.web]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Web {
    /// Activo.
    pub enabled: bool,
    /// Palabra clave → URL con `{q}` (RF-091).
    pub engines: BTreeMap<String, String>,
}

impl Default for Web {
    fn default() -> Self {
        let engines = [
            ("g", "https://www.google.com/search?q={q}"),
            ("ddg", "https://duckduckgo.com/?q={q}"),
            ("yt", "https://www.youtube.com/results?search_query={q}"),
            ("gh", "https://github.com/search?q={q}"),
            ("wiki", "https://es.wikipedia.org/w/index.php?search={q}"),
            ("crate", "https://crates.io/search?q={q}"),
            ("docs", "https://docs.rs/releases/search?query={q}"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        Self {
            enabled: true,
            engines,
        }
    }
}

/// `[plugins.shell]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Shell {
    /// Activo.
    pub enabled: bool,
    /// Prefijo (RF-060).
    pub prefix: String,
    /// `pwsh`, `powershell`, `cmd` o `nu`.
    pub shell: String,
    /// Mantener la terminal abierta tras el comando.
    pub keep_open: bool,
    /// Pedir confirmación para apagar, reiniciar, etc. (RF-062).
    pub confirm_system: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: ">".into(),
            shell: "pwsh".into(),
            keep_open: true,
            confirm_system: true,
        }
    }
}

/// `[plugins.generators]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Generators {
    /// Activo.
    pub enabled: bool,
    /// Longitud por defecto de las contraseñas.
    pub password_length: usize,
    /// Incluir símbolos en las contraseñas.
    pub password_symbols: bool,
}

impl Default for Generators {
    fn default() -> Self {
        Self {
            enabled: true,
            password_length: 20,
            password_symbols: true,
        }
    }
}

/// `[plugins.files]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Files {
    /// Activo.
    pub enabled: bool,
    /// Prioridad.
    pub priority: f32,
    /// Carpetas indexadas (`~` = carpeta del usuario).
    pub roots: Vec<String>,
    /// Patrones glob excluidos (RF-042).
    pub exclude: Vec<String>,
    /// Indexar archivos y carpetas ocultos.
    pub include_hidden: bool,
    /// Caracteres mínimos para buscar archivos sin prefijo.
    pub min_chars: usize,
    /// Si Everything (voidtools) está abierto, buscar con él en todo el disco (RF-041).
    pub everything: bool,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: -0.15,
            roots: [
                "~/Desktop",
                "~/Documents",
                "~/Downloads",
                "~/Pictures",
                "~/Videos",
                "~/Music",
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
            exclude: [
                "**/node_modules",
                "**/.git",
                "**/target",
                "**/__pycache__",
                "**/.venv",
                "**/AppData",
                "**/$RECYCLE.BIN",
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
            include_hidden: false,
            min_chars: 3,
            everything: true,
        }
    }
}

/// Ajustes comunes a todos los plugins (RF-205).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct PluginCommon {
    /// Activo.
    pub enabled: bool,
    /// Prioridad: se suma a la puntuación (−1..1).
    pub priority: f32,
}

impl Default for PluginCommon {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 0.0,
        }
    }
}

/// `[plugins.http]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Http {
    /// Activo.
    pub enabled: bool,
    /// Prioridad: se suma a la puntuación (−1..1).
    pub priority: f32,
    /// Atajo global que abre el mini Postman (`ctrl+alt+p`). Vacío: ninguno.
    pub hotkey: String,
}

impl Default for Http {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 0.0,
            hotkey: String::new(),
        }
    }
}

/// `[plugins.apps]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Apps {
    /// Activo.
    pub enabled: bool,
    /// Prioridad.
    pub priority: f32,
    /// Alias → nombre de la app (RF-031).
    pub aliases: BTreeMap<String, String>,
    /// Incluir ejecutables del `PATH` (RF-030).
    pub include_path: bool,
    /// Perfiles de lanzamiento: app con argumentos y carpeta (RF-033).
    pub profiles: Vec<AppProfile>,
}

/// `[[plugins.apps.profiles]]` (RF-033)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AppProfile {
    /// Nombre visible.
    pub name: String,
    /// Nombre de la app (como en el menú Inicio), ruta de un ejecutable o comando del `PATH`.
    pub target: String,
    /// Argumentos.
    pub args: String,
    /// Carpeta de trabajo. Admite `~` y `%VAR%`.
    pub cwd: String,
    /// Alias exactos.
    pub alias: Vec<String>,
    /// Abrir siempre como administrador.
    pub admin: bool,
}

impl Default for Apps {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 0.0,
            aliases: BTreeMap::new(),
            include_path: true,
            profiles: Vec::new(),
        }
    }
}

impl Apps {
    /// Ajustes comunes.
    pub fn common(&self) -> PluginCommon {
        PluginCommon {
            enabled: self.enabled,
            priority: self.priority,
        }
    }
}
