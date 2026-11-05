# Guía de usuario de Rukes

Rukes es un launcher de teclado para Windows 10 (22H2) y Windows 11. Pulsa `Alt+Espacio`,
escribe y pulsa `Enter`.

> La documentación completa, con ejemplos de código coloreados, está en HTML:
> [guía de usuario](index.html) (atajos, scripts, Rhai, workflows, triggers, plugins) y
> [documentación para desarrolladores](desarrolladores.html) (cómo funciona por dentro y cómo
> ampliarlo). Esta página es el resumen.

## Instalación

- **Instalador** (`Rukes-<versión>-setup-x64.exe`): se instala para tu usuario en
  `%LOCALAPPDATA%\Programs\Rukes`, sin permisos de administrador. Al desinstalar se conserva
  tu configuración.
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
| `Ctrl+S` | Fijar en el inicio una app, archivo, carpeta, script o workflow (en el portapapeles, fija el elemento) |
| `Ctrl+1` … `Ctrl+9` | Abrir el resultado N (con la búsqueda vacía, el fijado N) |
| `Alt+↑` / `Alt+↓` | Cambiar de puesto un fijado |
| `Ctrl+P` | Vista previa o detalles |
| `↑` `↓`, `Tab` / `Shift+Tab`, `Ctrl+J` `Ctrl+K` | Moverse por los resultados (al revés con la ventana anclada abajo) |
| `RePág` / `AvPág` | Una página arriba / abajo |
| `Esc` | Ocultar |

En el panel de ajustes: `Ctrl+S` guarda y `Esc` cierra. Los atajos del mini Postman están en
[su sección](#peticiones-http-mini-postman). Los workflows, los scripts (`@rukes.hotkey`) y el
mini Postman (`[plugins.http] hotkey`) pueden tener su propio atajo global.

Rukes aprende: lo que eliges más a menudo para una búsqueda sube en la lista.

### La ventana

- **Encabezado:** la búsqueda y, a su derecha, los iconos **＋ Nuevo** y **⚙ Ajustes** (su nombre
  sale al pasar el ratón).
- **Leyenda «SUGERENCIAS»:** con la búsqueda vacía, comandos de ejemplo en dos columnas
  (comando a la izquierda, qué hace a la derecha). Un clic lo escribe. En los ajustes (Ventana →
  Leyenda de comandos) eliges cuántos (6 por defecto) y cuáles; en `config.toml` son
  `hints_count` y `hints_items` de `[window]`, y `hints = false` la quita.
- **Filas:** icono, título y subtítulo cortados con «…» (las rutas por el medio). En la fila
  seleccionada o bajo el ratón aparecen **botones** con las demás acciones. Las tareas muestran
  su estado como etiqueta.
- **Grupos:** si la lista mezcla tipos, separadores finos con el nombre (APLICACIONES, ARCHIVOS…).
- **Pie de atajos:** la acción de `Enter` de la fila seleccionada y hasta dos atajos que le
  apliquen (`Ctrl+↵` Administrador, `Ctrl+P` Vista previa o Detalles).

**Crear cosas nuevas:** escribe `nuevo` (o `new`, o pulsa el icono ＋) y elige qué
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
| `http` | Abre el panel vacío; debajo, el historial de la sesión |
| `curl -X POST https://x.com -H 'A: b' -d '{}'` | Interpreta el curl (método, URL, cabeceras y cuerpo) |

Nada se envía hasta pulsar Enter. Al pulsarlo se abre el **panel del mini Postman**: una barra
con el método, la URL y «Enviar», que se maneja solo con el teclado:

| Tecla | Hace |
| --- | --- |
| `Enter` (en la URL) / `Ctrl+Enter` (en cualquier sitio) | Envía |
| `Tab` / `Shift+Tab` | Despliega y recorre Headers, Body, Auth y Params; tras la última se pliega |
| `↑` / `↓` (en la URL) | Peticiones recientes; `↓` al final devuelve lo que estabas escribiendo |
| `Ctrl+M` / `Ctrl+Shift+M` | Método siguiente / anterior |
| `Ctrl+L` | Vuelve a la URL |
| `Ctrl+H` | Respuesta: cuerpo ↔ cabeceras |
| `Ctrl+Shift+C` / `Ctrl+Shift+H` / `Ctrl+Shift+X` | Copia el cuerpo / las cabeceras / la petición como curl |
| `RePág` / `AvPág` | Desplaza la respuesta |
| `Esc` | Cierra el panel y vuelve a la búsqueda |

- Pegar una orden `curl` en la barra (también las copiadas de las herramientas del navegador,
  con `\` o `^` al final de línea) rellena el método, la URL, las cabeceras y el cuerpo.
- Headers: una por línea (`Nombre: valor`). Params: una por línea (`clave=valor`), se añaden a la
  URL. Auth: `Bearer <token>`, `Basic usuario:clave`, `usuario:clave` o un token suelto.
- La respuesta muestra arriba el estado (verde 2xx, azul 3xx, naranja 4xx, rojo 5xx), el tiempo
  en ms y el tamaño, y debajo el cuerpo (JSON con sangrado), que se puede seleccionar.
- `[plugins.http] hotkey = "ctrl+alt+p"` (o «Atajo del mini Postman» en los ajustes) abre el
  panel directamente desde cualquier sitio.

El historial solo vive en memoria (puede contener tokens) y se borra al cerrar Rukes.
`[plugins.http] enabled = false` lo quita.

### Automatización

- **Scripts:** archivos en la carpeta `scripts` junto a `config.toml` (PowerShell, Python,
  JavaScript, Rhai, .bat…) con una cabecera `@rukes.name`. `new script Backup` crea uno desde
  una plantilla; `logs` muestra el historial.

  ```powershell
  # @rukes.name   Saludar
  # @rukes.alias  hola
  # @rukes.arg    nombre:text:default=mundo
  # @rukes.output notification
  param([string]$nombre)
  Write-Output "Hola, $nombre"
  ```

  Cabeceras: `name`, `description`, `icon`, `alias`, `arg` (`nombre:tipo:opciones`), `output`
  (`notification`, `launcher`, `clipboard`, `terminal`, `silent`), `timeout`, `hotkey`, `admin`,
  `cwd`, `interpreter`, `filter`. Variables: `RUKES_QUERY`, `RUKES_ARG_<NOMBRE>`,
  `RUKES_CLIPBOARD`, `RUKES_SELECTED_FILE`, `RUKES_ACTIVE_WINDOW`… Con `output launcher`, un JSON
  `{"items": [{"title", "subtitle", "open" | "copy" | "query"}]}` se muestra como resultados.
- **Rhai:** scripts `.rhai` que corren dentro de Rukes, sin instalar nada, con una API segura:
  `http_get`, `http`, `run`, `open`, `clipboard`, `notify`, `windows`, `move_window`,
  `note_add`, `todo_add`, `remind`, `plugin_call`, `parse_json`… Si el script termina con una
  lista de mapas con `title`, cada uno es una fila.

  ```rhai
  // @rukes.name   Mi IP pública
  // @rukes.alias  mi ip
  // @rukes.output clipboard
  let ip = http_get("https://api.ipify.org");
  ip.trim();
  ip
  ```

- **Workflows:** `modo trabajo` abre apps y webs y las coloca en tus monitores. Se definen en
  la carpeta `workflows` (`[workflow.id]` con `name`, `alias`, `hotkey` y `steps`: `app`, `url`,
  `script`, `wait`, `theme`, `power_plan`, `kill`, `notify`; `window = { monitor, snap }`).
- **Triggers:** ejecutan un workflow o script solos. `triggers` lista los próximos; se pausan
  desde la bandeja.

  ```toml
  [[trigger]]
  at = "09:00"                 # o every = "30m", o on = "startup" | "login"
  days = ["laborables"]
  run = "workflow:trabajo"     # o "script:nombre"
  ```

- **Fondos de pantalla:** `wall`, `wall random`.
- **Plugins externos:** `nuevo plugin Mi bot` crea uno que ya funciona (Python, Node.js o
  PowerShell). Son programas que hablan JSON por stdin/stdout, una línea por mensaje
  (`initialize`, `query`, `action`, `cancel`); `plugins` lista los instalados. Ver la
  [guía HTML](index.html#plugins) y el [protocolo](plugin-protocol.md).

## Configuración

- **Panel de ajustes:** escribe `ajustes`, pulsa el icono ⚙ o elige «Ajustes…» en el icono de
  la bandeja. A la izquierda las pestañas (General, Ventana, Plugins, Extras); a la derecha los
  ajustes en tarjetas (Apariencia, Sistema, Tamaño, Leyenda de comandos, Comportamiento).
  `Guardar` (o `Ctrl+S`) escribe solo lo cambiado y conserva tus comentarios; está desactivado
  mientras no cambies nada. `Cancelar` o `Esc` vuelve a la búsqueda.
- **Archivo:** escribe `configuración` (o «Editar config.toml» en el panel) para abrir
  `config.toml` (`%APPDATA%\Rukes\config.toml`), donde está todo lo demás. Los cambios se
  aplican al guardar, sin reiniciar.
- **Posición:** `position = "center"`, `"top_center"` o `"bottom_center"` en `[window]`. Abajo,
  la barra queda junto a la barra de tareas y los resultados crecen hacia arriba, con el mejor
  pegado a la búsqueda (`↑` sube a los siguientes).
- **Autocompletado:** junto a `config.toml` se genera `config.schema.json`. Con VS Code y la
  extensión *Even Better TOML* tienes autocompletado, descripciones y validación.
- **Temas:** `theme = "system"` (por defecto: `rukes-light` o `rukes-dark` según Windows),
  `"rukes-dark"`, `"rukes-light"`, `"tokyo-night"`, `"catppuccin-mocha"`, `"nord"`, `"midnight"`
  (negro puro, sin transparencia)… `nuevo tema Mi tema` crea uno propio. Los colores son
  **tokens** (`bg`, `surface`, `surface_hover`, `border`, `text`, `text_muted`, `text_faint`,
  `accent`, `accent_soft`, `selection`); los que un tema no define se calculan a partir de los
  demás.
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
- **Un plugin externo aparece desactivado:** falló 3 veces seguidas. Mira el log, corrígelo y
  pulsa Enter sobre él en `plugins`.
- **Un trigger no se ejecutó:** están en pausa, el equipo estaba suspendido a esa hora o `days`
  no incluye hoy.
- **Logs detallados:** variable de entorno `RUKES_LOG=debug`.
