# Guía de usuario de Rukes

Rukes es un launcher de teclado para Windows 10 (22H2) y Windows 11. Pulsa `Alt+Espacio`,
escribe y pulsa `Enter`.

## Instalación

- **Instalador** (`Rukes-<versión>-setup-x64.exe`): se instala para tu usuario en
  `%LOCALAPPDATA%\Programs\Rukes`, sin permisos de administrador. Al desinstalar se conserva
  tu configuración.
- **winget**: `winget install HenryOsorio.Rukes` (cuando el paquete esté publicado).
- **Portable** (`Rukes-<versión>-portable-x64.zip`): descomprime en cualquier carpeta (o en
  un USB) y ejecuta `rukes.exe`. El archivo `portable.txt` hace que la configuración, la caché
  y los logs se guarden en la carpeta `data` junto al exe. Bórralo para usar `%APPDATA%\Rukes`.

Rukes queda residente con un icono en la bandeja. Para que arranque con Windows pon
`start_with_windows = true` en `[general]`.

## Lo básico

| Tecla | Acción |
| --- | --- |
| `Alt+Espacio` | Mostrar u ocultar (configurable en `[general] hotkey`) |
| `Enter` | Ejecutar la acción principal |
| `Ctrl+Enter` | Abrir como administrador |
| `Ctrl+Shift+Enter` | Abrir la carpeta contenedora |
| `Ctrl+C` | Copiar la ruta o el valor |
| `Ctrl+O` / `Ctrl+T` | Abrir con… / terminal en la carpeta |
| `Ctrl+E` / `Ctrl+D` | Editar / borrar (scripts, snippets, historial, recordatorios) |
| `Ctrl+P` | Vista previa |
| `↑` `↓`, `Tab`, `Ctrl+J` `Ctrl+K` | Moverse por los resultados |
| `Esc` | Ocultar |

Rukes aprende: lo que eliges más a menudo para una búsqueda sube en la lista.

## Qué puedes escribir

### Apps y archivos

- `chr` → Google Chrome. Las apps se reindexan solas al instalar o quitar programas (accesos
  directos del menú Inicio).
- `f: informe` solo archivos, `d: proyectos` solo carpetas, `contrato .pdf` por extensión.
- Si tienes [Everything](https://www.voidtools.com) abierto, la búsqueda de archivos lo usa
  para buscar en todo el disco al instante. Sin él se usa el índice propio de las carpetas de
  `[plugins.files] paths`. Se desactiva con `everything = false`.

**Perfiles de apps:** una app con argumentos fijos y alias propios.

```toml
[[plugins.apps.profiles]]
name = "Code en el proyecto"
target = "Visual Studio Code"      # nombre de una app indexada o una ruta
args = "-n C:/proyectos/web"
cwd = "C:/proyectos/web"
alias = ["cw"]
admin = false
```

### Calculadora, unidades, divisas y colores

- `= 2^10`, `5 km a millas`, `70 F en C` (sin `=` también, si parece una operación).
- `100 usd a eur`, `50 eur en cop`: conversión de divisas. Rukes no se conecta a internet por
  defecto: activa `currency_online = true` en `[plugins.calc]` para descargar las tasas de
  open.er-api.com (como mucho dos veces al día). Se guardan en caché y sirven sin conexión.
- `#ff5733`, `rgb(255 87 51)`, `hsl(11 100% 60%)`: muestra el color y sus formatos HEX, RGB,
  HSL y HSV. `Enter` copia.

### Controles del sistema

| Escribe | Hace |
| --- | --- |
| `vol`, `vol 50`, `vol +10`, `vol mute` | Volumen |
| `brillo`, `brillo 70` | Brillo de la pantalla del portátil |
| `wifi`, `wifi off`, `bluetooth on` | Radios |
| `avion` | Modo avión (apaga o enciende todas las radios) |
| `oscuro`, `claro` | Tema de Windows |

### Web, terminal y sistema

- `github.com`, `g rust lifetimes`, `yt lofi`: abrir URL o buscar (motores en `[plugins.web]`).
- `> dir`, `> shell:startup`: comando en la terminal o URI del shell.
- `bloquear`, `apagar`, `reiniciar`, `vaciar papelera` (con confirmación).
- `win code` ventanas abiertas, `kill chrome` / `ps` procesos.
- `hora en Tokio`, `días hasta 25 dic`.
- `uuid`, `pass 24`, `sha256 texto`, `b64 texto`: generadores y codificadores.

### Productividad

- `clip`, `clip factura`: historial del portapapeles, cifrado con DPAPI. `Enter` pega en la
  ventana anterior, `Ctrl+S` fija un elemento. No guarda lo que los gestores de contraseñas
  marcan como sensible ni las apps de `[plugins.clipboard] exclude`.
- `snip`, `firma Ana`: snippets con variables (`{fecha}`, `{cursor}`, `{campo:Nombre}`).
- `note idea`, `todo comprar pan`, `todo`: notas y tareas en Markdown.
- `remind 15m llamar a Ana`, `recordar 14:30 reunión`: recordatorios.

### Automatización

- **Scripts:** archivos en la carpeta `scripts` junto a `config.toml` (PowerShell, Python,
  Rhai, .bat…). `new script Backup` crea uno desde una plantilla; `logs` muestra el historial.
- **Workflows:** `modo trabajo` abre apps y webs y las coloca en tus monitores. Se definen en
  la carpeta `workflows`. `triggers` lista los disparadores programados (se pausan desde la
  bandeja).
- **Fondos de pantalla:** `wall`, `wall random`.
- **Plugins externos:** `plugins` lista los instalados. Cualquier programa que hable JSON por
  stdin/stdout sirve; ver [protocolo](plugin-protocol.md).

## Configuración

- Escribe `configuración` en Rukes para abrir `config.toml` (`%APPDATA%\Rukes\config.toml`).
  Los cambios se aplican al guardar, sin reiniciar.
- **Autocompletado:** junto a `config.toml` se genera `config.schema.json`. Con VS Code y la
  extensión *Even Better TOML* tienes autocompletado, descripciones y validación.
- **Temas:** `theme = "tokyo-night"`, `"catppuccin-mocha"`, `"nord"`, `"system"`… Copia un
  archivo de `themes` a la carpeta `themes` de tu configuración para crear uno propio.
- **Idioma:** `language = "auto"`, `"es"` o `"en"`.

### Exportar e importar

- `exportar configuración` copia tu configuración (config, temas, scripts, snippets, notas,
  workflows, plugins) a `Documentos\Rukes-config-<fecha>`. No incluye la caché ni el historial.
- `importar C:\ruta\Rukes-config-...` la restaura. Antes guarda una copia de la actual en
  `Rukes-backup-<fecha>` junto a la carpeta de configuración.
- Desde la línea de comandos: `rukes.exe --export <carpeta>`, `rukes.exe --import <carpeta>`,
  `rukes.exe --schema <archivo>`.

## Carpetas

| Qué | Dónde |
| --- | --- |
| Configuración, scripts, snippets, notas, workflows, plugins | `%APPDATA%\Rukes` |
| Caché, índice, historial del portapapeles | `%LOCALAPPDATA%\Rukes` |
| Logs | `%LOCALAPPDATA%\Rukes\logs` |

En modo portable todo va a la carpeta `data` junto a `rukes.exe`.

## Problemas frecuentes

- **`Alt+Espacio` no abre Rukes:** otra app usa el atajo. Cambia `hotkey` en `[general]`;
  el log indica si no se pudo registrar.
- **Una app nueva no aparece:** espera unos segundos tras instalarla o escribe `recargar`
  («Recargar Rukes» vuelve a indexar las aplicaciones).
- **Brillo no disponible:** solo funciona en pantallas internas (portátiles); los monitores
  externos no exponen el brillo a Windows.
- **Logs detallados:** variable de entorno `RUKES_LOG=debug`.
