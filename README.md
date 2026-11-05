# Rukes

Launcher de productividad y automatización para Windows 10/11, escrito en Rust: ligero, rápido y con la automatización como ciudadano de primera clase.

> Estado: **Fase 2**: apps, archivos, calculadora, web, shell, ventanas, procesos y generadores con `Alt+Space`. La especificación completa está en [`Rukes Contexto y Requerimientos.md`](Rukes%20Contexto%20y%20Requerimientos.md).

## Compilar

Requisitos: Rust estable (toolchain MSVC) en Windows 10 22H2+ o Windows 11.

```bash
cargo build --release -p rukes-app
```

El binario queda en `target/release/rukes.exe`. Nivel de log con `RUKES_LOG` (por ejemplo `RUKES_LOG=debug`).

## Uso

1. Ejecuta `rukes.exe`. Queda residente, con un icono en la bandeja.
2. Pulsa `Alt+Space` y escribe (`chr` → Google Chrome).

| Escribe | Resultado |
| --- | --- |
| `chr`, `informe` | Apps y archivos de tus carpetas |
| `f: informe`, `d: proyectos`, `contrato .pdf` | Solo archivos, solo carpetas, por extensión |
| `= 5 km a millas`, `2^10`, `70 F en C` | Calculadora y conversiones (sin `=` también, si parece una operación) |
| `hora en Tokio`, `días hasta 25 dic` | Hora y fechas |
| `github.com`, `g rust lifetimes`, `yt lofi` | Abrir URL o buscar en la web |
| `> dir`, `> shell:startup` | Comando en la terminal o URI del shell |
| `bloquear`, `apagar`, `vaciar papelera` | Comandos del sistema (con confirmación) |
| `win code`, `kill chrome`, `ps` | Ventanas abiertas y procesos |
| `uuid`, `pass 24`, `sha256 texto`, `b64 texto` | Generadores, hashes y codificadores (Enter copia) |
| `gh issue Mi bug`, `hola Ana` | Tus scripts, con argumentos en línea |
| `new script Backup`, `logs` | Crear un script desde una plantilla, historial de ejecuciones |
| `clip`, `clip factura` | Historial del portapapeles: Enter pega en la ventana anterior |
| `firma Ana`, `snip` | Snippets con variables y campos (`{fecha}`, `{cursor}`, `{campo:Nombre}`) |
| `note idea`, `todo comprar pan`, `todo` | Notas y tareas en Markdown (Enter completa una tarea) |
| `remind 15m llamar a Ana`, `recordar 14:30 reunión` | Recordatorios con notificación |
| `modo trabajo`, `workflows`, `triggers` | Workflows (apps y webs colocadas en tus monitores) y triggers programados |

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

- **Configuración:** `%APPDATA%\Rukes\config.toml`, que se crea comentado en el primer arranque y
  se aplica al guardar. También puedes abrirla escribiendo «configuración» en el launcher.
- **Temas:** `theme` en `[general]` (`system`, `tokyo-night`, `catppuccin-mocha`, `nord`…).
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
- **Workflows y triggers:** archivos `.toml` de la carpeta `workflows` junto a `config.toml`
  (la primera vez se crean ejemplos comentados). Los triggers se pausan desde la bandeja.
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

## Licencia

MIT o Apache-2.0, a elección.
