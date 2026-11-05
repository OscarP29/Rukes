# Rukes

Launcher de productividad y automatización para Windows 10/11, escrito en Rust: ligero, rápido y con la automatización como ciudadano de primera clase.

> **Versión 1.0.** Documentación en una página: [`docs/index.html`](docs/index.html) (ábrela en
> el navegador). Guía de usuario completa en [`docs/usuario.md`](docs/usuario.md); la
> especificación en [`Rukes Contexto y Requerimientos.md`](Rukes%20Contexto%20y%20Requerimientos.md).

## Capturas

| Búsqueda | Calculadora y colores | Portapapeles |
| --- | --- | --- |
| ![Búsqueda de apps](docs/img/busqueda.png) | ![Calculadora](docs/img/calculadora.png) | ![Portapapeles](docs/img/portapapeles.png) |

| Controles del sistema | Fondos de pantalla | Temas |
| --- | --- | --- |
| ![Controles](docs/img/controles.png) | ![Fondos](docs/img/wall.png) | ![Temas](docs/img/temas.png) |

## Instalar

- **Instalador:** `Rukes-<versión>-setup-x64.exe` de la página de releases. Se instala para tu
  usuario, sin administrador.
- **winget:** `winget install HenryOsorio.Rukes`.
- **Portable:** descomprime `Rukes-<versión>-portable-x64.zip` donde quieras; con
  `portable.txt` todo se guarda en la carpeta `data` junto al exe.

## Compilar

Requisitos: Rust estable (toolchain MSVC) en Windows 10 22H2+ o Windows 11.

```bash
cargo build --release -p rukes-app
```

El binario queda en `target/release/rukes.exe`. Nivel de log con `RUKES_LOG` (por ejemplo `RUKES_LOG=debug`).
Para generar el ZIP, el instalador y los manifiestos de winget, ver [`docs/release.md`](docs/release.md).

## Uso

1. Ejecuta `rukes.exe`. Queda residente, con un icono en la bandeja.
2. Pulsa `Alt+Space` y escribe (`chr` → Google Chrome).

| Escribe | Resultado |
| --- | --- |
| `chr`, `informe` | Apps y archivos de tus carpetas |
| `f: informe`, `d: proyectos`, `contrato .pdf` | Solo archivos, solo carpetas, por extensión |
| `= 5 km a millas`, `2^10`, `70 F en C` | Calculadora y conversiones (sin `=` también, si parece una operación) |
| `100 usd a eur`, `#ff5733`, `hsl(11 100% 60%)` | Divisas (con caché sin conexión) y colores en HEX, RGB, HSL y HSV |
| `vol 50`, `brillo 70`, `wifi off`, `avion`, `oscuro` | Controles del sistema |
| `hora en Tokio`, `días hasta 25 dic` | Hora y fechas |
| `github.com`, `g rust lifetimes`, `yt lofi` | Abrir URL o buscar en la web |
| `> dir`, `> shell:startup` | Comando en la terminal o URI del shell |
| `bloquear`, `apagar`, `vaciar papelera` | Comandos del sistema (con confirmación) |
| `win code`, `kill chrome`, `ps` | Ventanas abiertas y procesos |
| `uuid`, `pass 24`, `sha256 texto`, `b64 texto` | Generadores, hashes y codificadores (Enter copia) |
| `gh issue Mi bug`, `hola Ana` | Tus scripts, con argumentos en línea |
| `new script Backup`, `logs` | Crear un script desde una plantilla, historial de ejecuciones |
| `nuevo`, `nuevo tema Mi tema`, `nuevo plugin Mi bot` | Crear un tema, workflow, script o plugin: pregunta el nombre y el lenguaje y abre el archivo |
| `clip`, `clip factura` | Historial del portapapeles: Enter pega en la ventana anterior |
| `firma Ana`, `snip` | Snippets con variables y campos (`{fecha}`, `{cursor}`, `{campo:Nombre}`) |
| `note idea`, `todo comprar pan`, `todo` | Notas y tareas en Markdown (Enter completa una tarea, Ctrl+D borra una nota) |
| `remind 15m llamar a Ana`, `recordar 14:30 reunión` | Recordatorios con notificación |
| `modo trabajo`, `workflows`, `triggers` | Workflows (apps y webs colocadas en tus monitores) y triggers programados |
| `wall`, `wall random` | Fondos de pantalla con miniaturas y previsualización; cambio automático opcional |
| `plugins` | Plugins externos instalados, su estado y permisos |
| `ajustes` | Panel visual de ajustes: tema, posición, ventana y plugins |
| `help`, `help vol`, `info calc` | Ayuda de uso de cualquier comando, con ejemplos que se prueban con Enter |
| `http 3000/api`, `post :8080/login {"u":"ana"}` | Mini Postman: panel de teclado con método, URL, pestañas Headers/Body/Auth/Params, historial y pegado de `curl` |

| Tecla | Acción |
| --- | --- |
| `Enter` | Abrir |
| `Ctrl+Enter` | Abrir como administrador |
| `Ctrl+Shift+Enter` | Abrir la carpeta contenedora |
| `Ctrl+C` | Copiar la ruta |
| `Ctrl+O` | Abrir con… (archivos) |
| `Ctrl+T` | Abrir una terminal en la carpeta |
| `Ctrl+E` | Editar el script, el snippet o el archivo de tareas |
| `Ctrl+D` | Mover el script a la papelera (con confirmación), borrar del historial, cancelar un recordatorio |
| `Ctrl+S` | Fijar o soltar un elemento del historial del portapapeles |
| `Ctrl+P` | Mostrar u ocultar la vista previa |
| `↑` `↓` / `Tab` / `Ctrl+J` `Ctrl+K` | Moverse por los resultados |
| `Esc` | Ocultar |

- **Ajustes:** escribe «ajustes» (o usa el engranaje de la leyenda o la bandeja) para cambiar
  tema, posición, atajo y plugins sin tocar archivos.
- **Configuración:** `%APPDATA%\Rukes\config.toml`, que se crea comentado en el primer arranque y
  se aplica al guardar. También puedes abrirla escribiendo «configuración» en el launcher.
- **Temas:** `theme` en `[general]` (`system`, `tokyo-night`, `catppuccin-mocha`, `nord`,
  `midnight` (negro puro)…).
  Copia un archivo de `themes/` a la carpeta `themes` junto a `config.toml` para crear el tuyo;
  se aplica al guardar.
- **Idioma:** `language = "auto"`, `"es"` o `"en"`. Los textos están en
  `crates/rukes-core/lang`; un archivo en la carpeta `lang` de la configuración los cambia o
  añade un idioma.
- **Portapapeles:** el historial se guarda cifrado (DPAPI) en `%LOCALAPPDATA%\Rukes\clipboard`.
  No registra lo que los gestores de contraseñas marcan como sensible ni las apps de
  `[plugins.clipboard] exclude`; `enabled = false` deja de escuchar el portapapeles.
- **Snippets, notas y tareas:** carpeta `snippets` y archivos `notes/notas.md` y
  `notes/tareas.md` junto a `config.toml` (configurables; sirven en Obsidian).
- **Plugins externos:** cualquier programa que hable JSON por stdin/stdout, en una carpeta con
  `plugin.toml` dentro de `plugins` junto a `config.toml`. Se instalan y quitan en caliente.
  Protocolo en [`docs/plugin-protocol.md`](docs/plugin-protocol.md); ejemplos en Python y Rust en
  [`plugins-examples`](plugins-examples).
- **Workflows y triggers:** archivos `.toml` de la carpeta `workflows` junto a `config.toml`
  (la primera vez se crean ejemplos comentados). Los triggers se pausan desde la bandeja.
- **Autocompletado:** junto a `config.toml` se genera `config.schema.json` (VS Code con Even
  Better TOML lo usa para autocompletar y validar).
- **Perfiles de apps:** `[[plugins.apps.profiles]]` lanza una app con argumentos, carpeta y
  alias propios. Las apps se reindexan solas al cambiar el menú Inicio.
- **Everything:** si está abierto, la búsqueda de archivos lo usa (`everything = false` en
  `[plugins.files]` lo desactiva).
- **Exportar e importar:** «exportar configuración» e «importar <carpeta>», o
  `rukes.exe --export <carpeta>` / `--import <carpeta>`.
- **Salir:** escribe «salir» en el launcher o usa el menú del icono de la bandeja.
- **Iniciar con Windows:** `start_with_windows = true` en `[general]`.
- **Logs:** `%LOCALAPPDATA%\Rukes\logs`.

## Estructura

| Ruta | Contenido |
| --- | --- |
| `crates/rukes-core` | Motor: búsqueda, ranking, trait `Plugin`, acciones |
| `crates/rukes-plugins` | Plugins integrados |
| `crates/rukes-script` | Scripts de usuario y Rhai |
| `crates/rukes-config` | Configuración, temas, hot reload |
| `crates/rukes-platform` | API de Windows (único crate con `unsafe`) |
| `crates/rukes-ui` | Ventana, render, animaciones |
| `crates/rukes-app` | Binario `rukes.exe` |
| `scripts-examples/` | Scripts de ejemplo |
| `themes/` | Temas incluidos (TOML) |
| `spikes/` | Prototipos desechables de la fase 0 (workspace aparte) |
| `docs/decisions/` | ADRs |
| `packaging/`, `scripts/` | Instalador (Inno Setup), manifiestos de winget, empaquetado y firma |

## Licencia

MIT o Apache-2.0, a elección.
