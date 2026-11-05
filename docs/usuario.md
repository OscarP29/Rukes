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

Con la búsqueda vacía aparece una **leyenda** con ejemplos: haz clic en uno para escribirlo.
En la fila seleccionada (o bajo el ratón) aparecen **botones** con las demás acciones (abrir
la carpeta, copiar la ruta, como administrador, ver detalles…); pasa el ratón por encima para
ver qué hace cada uno. En los ajustes (pestaña Ventana) eliges cuántos comandos muestra la
leyenda y cuáles; en `config.toml` son `hints_count` y `hints_items` de `[window]`, y
`hints = false` la quita.

**Crear cosas nuevas:** escribe `nuevo` (o `new`, o pulsa «Nuevo» en la leyenda) y elige qué
crear: un tema, un workflow, un script o un plugin. Rukes te pide el nombre y, si hace falta,
el lenguaje (scripts y plugins) o el tema en que basarte; crea el archivo en su carpeta y lo
abre en el editor. Un tema nuevo se activa al momento. También directo: `nuevo tema Mi tema`,
`nuevo workflow Modo foco`, `nuevo plugin Mi bot`, `nuevo script Backup`.

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
- `note idea`, `todo comprar pan`, `todo`: notas y tareas en Markdown. En la lista de `note`,
  Ctrl+D borra la nota seleccionada.
- `remind 15m llamar a Ana`, `recordar 14:30 reunión`: recordatorios.

### Ayuda

- `help` (o `info`, `ayuda`) lista todos los comandos con un resumen.
- `help vol`, `info calc`, `help http`: la ayuda de un comando, con ejemplos. Enter en un
  ejemplo lo escribe en el launcher para probarlo; `Ctrl+P` muestra la ayuda completa.

### Peticiones HTTP (mini Postman)

| Escribe | Envía |
| --- | --- |
| `http 3000` | `GET http://localhost:3000` (un número es un puerto local) |
| `get :8080/api/users` | `GET http://localhost:8080/api/users` |
| `post 3000/login {"user":"ana"}` | `POST` con cuerpo JSON (`Content-Type` automático) |
| `put 3000/u/1 -H "Authorization: Bearer x" {"a":1}` | Con cabeceras (`-H`, repetible) |
| `http api.github.com/users/octocat` | Sin esquema: `https` (y `http` para localhost e IP) |
| `http` | Historial de la sesión (Enter repite) |

Nada se envía hasta pulsar Enter. La respuesta muestra estado, tiempo y tamaño, y el cuerpo
(JSON formateado) en el panel de detalles. Enter sobre el estado repite la petición; sobre el
cuerpo, las cabeceras o «Copiar como curl», los copia. El historial solo vive en memoria
(puede contener tokens) y se borra al cerrar Rukes. `[plugins.http] enabled = false` lo quita.

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

- **Panel de ajustes:** escribe `ajustes`, pulsa «Ajustes» en la leyenda o elige «Ajustes…» en
  el icono de la bandeja. Tiene pestañas para lo general (tema, idioma, atajo, inicio con
  Windows), la ventana (posición, ancho, resultados, leyenda, fondo), los plugins y los extras
  (divisas, Everything…). `Guardar` (o `Ctrl+S`) escribe solo lo cambiado y conserva tus
  comentarios; `Esc` vuelve a la búsqueda.
- **Archivo:** escribe `configuración` (o «Editar config.toml» en el panel) para abrir
  `config.toml` (`%APPDATA%\Rukes\config.toml`), donde está todo lo demás. Los cambios se
  aplican al guardar, sin reiniciar.
- **Posición:** `position = "center"`, `"top_center"` o `"bottom_center"` en `[window]`. Abajo,
  la barra queda junto a la barra de tareas y los resultados crecen hacia arriba, con el mejor
  pegado a la búsqueda (`↑` sube a los siguientes).
- **Autocompletado:** junto a `config.toml` se genera `config.schema.json`. Con VS Code y la
  extensión *Even Better TOML* tienes autocompletado, descripciones y validación.
- **Temas:** `theme = "tokyo-night"`, `"catppuccin-mocha"`, `"nord"`, `"midnight"` (negro
  puro, sin transparencia), `"system"`… Copia un
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
