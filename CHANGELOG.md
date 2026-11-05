# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/); versionado [SemVer](https://semver.org/lang/es/).

## [Unreleased]

### Fase 6: workflows y automatización

#### Añadido
- Workflows en archivos `.toml` de la carpeta `workflows` (vigilada; la primera vez se crean
  los modos de ejemplo Trabajo, Gaming, Presentación y Proyecto X). Pasos: `app`, `url`,
  `script`, `wait`, `theme`, `power_plan`, `kill` y `notify`.
- Posición de las ventanas por paso: monitor, `snap` (mitades, cuartos, tercios, centro,
  completa), coordenadas y tamaño en píxeles o porcentaje, o maximizada. Se espera a que aparezca
  la ventana nueva y se compensa el borde invisible de Windows 10/11.
- Cada workflow es un resultado del launcher (nombre y alias) con vista previa de sus pasos
  (`Ctrl+P`), atajo global propio (`hotkey`) e informe por paso al terminar.
- Triggers `[[trigger]]`: a una hora (con días de la semana), cada intervalo, al iniciar sesión
  o al arrancar Rukes; ejecutan un workflow o un script. `triggers` muestra la próxima
  ejecución. Se pausan desde la bandeja o con `[workflows] triggers = false`.
- Atajos globales de los scripts (`@rukes.hotkey` en la cabecera).
- `[workflows]` en la configuración.

#### Pendiente
- Pasos condicionales (RF-153) y triggers por evento (RF-161), prioridad C.

### Fase 5: portapapeles, snippets y notas

#### Añadido
- Historial del portapapeles (`clip`): texto copiado con búsqueda, origen y antigüedad; Enter
  pega en la ventana anterior (o copia con `paste = false`), `Ctrl+S` fija y `Ctrl+D` borra.
  Los fijados no caducan ni cuentan para `max_items`; `expire_days` configurable.
- No se registra el contenido que Windows marca como sensible
  (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory = 0`,
  `Clipboard Viewer Ignore`) ni lo copiado desde las apps de `exclude` (gestores de
  contraseñas por defecto).
- Historial cifrado con DPAPI en `%LOCALAPPDATA%\Rukes\clipboard\history.bin`. Con el plugin
  desactivado no se escucha el portapapeles.
- Snippets en `.toml` (`[[snippet]]`) o `.md` (cabecera opcional) en la carpeta `snippets`,
  vigilada. Variables `{fecha}`, `{hora}`, `{fechahora}`, `{portapapeles}`, `{cursor}` y campos
  `{campo:Nombre}` que se rellenan tras la palabra clave (`firma Ana`). Se insertan en la
  ventana anterior y el portapapeles vuelve a su contenido anterior.
- `note`/`nota`, `todo`/`tarea` y `remind`/`recordar`: notas con fecha y tareas en Markdown
  compatible con Obsidian; recordatorios (`15m`, `1h30m`, `14:30`) con notificación de la
  bandeja, persistentes, que avisan al arrancar si vencieron con Rukes cerrado.
- `[plugins.clipboard]`, `[plugins.snippets]` y `[plugins.notes]` en la configuración.
- Escribir solo la palabra clave de estos plugins (`clip`, `todo`, `recordar`) muestra su lista
  arriba, junto a la búsqueda global.

#### Pendiente
- Expansión automática global de snippets (RF-113, prioridad C) e imágenes en el historial.

### Fase 4: temas, previsualización e i18n

#### Añadido
- Temas en TOML (`[theme]`, `[theme.colors]`, `[theme.font]`, `[theme.layout]`): colores,
  fuentes, radio, márgenes, opacidad, tamaño de icono y alto de fila, con `extends` para cambiar
  solo algunos valores. Errores con línea y columna; se mantiene el último tema válido.
- Temas incluidos: Tokyo Night, Catppuccin (Latte, Frappé, Macchiato, Mocha), Gruvbox, Nord,
  Dracula y Rukes Light. `theme = "system"` (por defecto) usa `light_theme`/`dark_theme` según
  el modo de Windows.
- Carpeta `themes` junto a la configuración, vigilada: los temas se recargan al guardar.
- Animaciones de 80–150 ms al aparecer, al cambiar de selección y al cambiar de tamaño
  (`window.animations`); se apagan también si Windows tiene desactivadas las animaciones.
- Panel de previsualización a la derecha (`Ctrl+P` o `window.preview_panel`): miniatura,
  texto, Markdown y datos del archivo o carpeta.
- Caché de iconos en disco (`cache/icons`), invalidada por fecha y tamaño del archivo.
- Textos de la interfaz en `lang/es.toml` y `lang/en.toml` (incluidos en el binario); un archivo
  en la carpeta `lang` de la configuración cambia textos o añade un idioma sin recompilar, y se
  recarga al guardar.
- Respeta el escalado de texto y el contraste alto de Windows.
- Texto con ligaduras, respaldo de fuentes y emoji en color.

#### Cambiado
- `window.backdrop` pasa a `auto` (el del tema) por defecto; con contraste alto siempre es
  sólido. El fondo sólido (Windows 10) es opaco.
- Todas las cadenas bilingües del código pasan a claves de traducción.

### Fase 3: sistema de scripts

#### Añadido
- Carpeta de scripts vigilada (`[scripts] dir`, por defecto `scripts` junto a la configuración):
  todo `.ps1`, `.bat`/`.cmd`, `.py`, `.js`, `.ts`, `.lua`, `.rhai`, `.exe` o extensión mapeada
  en `[scripts.interpreters]` con cabecera `@rukes.*` aparece sin reiniciar. La primera vez se
  copian dos ejemplos.
- Cabecera de metadatos: nombre, descripción, icono, alias, argumentos, salida, timeout, atajo
  (se registrará en la fase 6), admin, directorio de trabajo, intérprete y `filter`.
- Argumentos con tipo (texto, número, opción, archivo, sí/no), obligatorios o con valor por
  defecto, escritos en línea tras el alias (`gh issue Mi bug`); las opciones se completan.
- Modos de salida: silencioso, notificación (toast desde el icono de la bandeja), launcher,
  portapapeles y terminal visible. `Ctrl+Enter` ejecuta como administrador.
- Tiempo máximo por script (o `default_timeout`) que termina todo el árbol de procesos (job
  object), y cancelación desde el resultado «Ejecutando…» o desde `logs`.
- `logs`: historial persistente de ejecuciones con estado, duración y salida.
- `new script [nombre]`: plantillas de PowerShell, Python, JavaScript, Batch y Rhai que se abren
  en el editor configurado.
- `Ctrl+E` edita un script y `Ctrl+D` lo mueve a la papelera (con confirmación).
- Rhai embebido con API segura: `open`, `run`, portapapeles, `notify`, `sleep`, HTTP (WinHTTP,
  sin dependencias), ventanas (`windows`, `focus_window`, `move_window`), `env`, JSON. Sin
  acceso a archivos, `eval` ni módulos; límites de tiempo y de operaciones.
- *Script filters*: scripts que devuelven resultados JSON mientras se escribe.
- Variables `RUKES_QUERY`, `RUKES_ARG_<NOMBRE>`, `RUKES_SELECTED_FILE(S)` (selección del
  Explorador), `RUKES_CLIPBOARD`, `RUKES_ACTIVE_WINDOW`, `RUKES_ACTIVE_PROCESS` y otras.
- `scripts-examples/` con ejemplos (incluido `gh issue`), `docs/checklists/fase-3.md`.

#### Cambiado
- Las palabras clave pueden tener varias palabras y gana la más larga: `gh issue x` va al
  script antes que a la búsqueda web `gh`.

### Fase 2: plugins esenciales

#### Añadido
- Calculadora (`=` y automática): operaciones, porcentajes, unidades con nombres en español
  (`5 km a millas`), temperaturas, hora en otras ciudades (`hora en Tokio`), días hasta una
  fecha, `hoy`/`ahora`. Usa `fend-core`.
- Web: URLs escritas directamente y búsquedas con palabra clave (`g`, `ddg`, `yt`, `gh`, `wiki`,
  `crate`, `docs`) configurables.
- Shell (`>`): comandos en `pwsh`/`powershell`/`cmd`/`nu` dentro de Windows Terminal o de la
  consola, como administrador con `Ctrl+Enter`, y URIs `shell:`/`ms-settings:`.
- Comandos del sistema: bloquear, suspender, hibernar, apagar, reiniciar, cerrar sesión y vaciar
  la papelera, con confirmación configurable.
- Ventanas abiertas: buscar por título o ejecutable y traerlas al frente (restaurando).
- Procesos (`kill`, `ps`): agrupados por ejecutable con RAM y CPU, cada proceso por separado,
  confirmación antes de terminar, opción de terminar el árbol y procesos del sistema protegidos.
- Generadores: UUID v4/v7, contraseñas, lorem ipsum, timestamps; hashes MD5, SHA-1, SHA-256,
  SHA-512 y CRC32 de texto o archivo; Base64, URL, HTML y hex.
- Archivos: índice propio compacto (12 bytes por entrada + nombre), caché binaria, watcher con
  actualización incremental por carpeta, exclusiones glob, `f:`/`d:` y filtro por extensión;
  acciones abrir, ubicación, copiar ruta, abrir con… (`Ctrl+O`) y terminal aquí (`Ctrl+T`).
- Icono en la bandeja con menú (Mostrar, Configuración, Recargar, Pausar/Reanudar atajo, Salir)
  e inicio con Windows (`start_with_windows`).
- Núcleo: `ActionOutcome::ShowResults` para confirmaciones y listas que devuelve una acción.
- Plantilla `config.toml` con todas las secciones nuevas; test que la compara con los valores
  por defecto.
- `docs/checklists/fase-2.md`, métricas de la fase 2 y benchmark `files_bench`.

#### Cambiado
- Una palabra clave solo dirige la búsqueda si va seguida de un espacio (`g` a secas sigue
  buscando apps; `g rust` busca en la web).

### Fase 1 — MVP: invocar, buscar, abrir

#### Añadido
- `rukes-core`: trait `Plugin`, motor con pool de hilos, `QueryId` y cancelación, aislamiento de
  pánicos, búsqueda difusa (`nucleo-matcher`), ranking con frecency de decaimiento exponencial y
  persistencia JSON atómica. Textos en español e inglés (`i18n::tr`).
- `rukes-config`: esquema de `config.toml` con valores por defecto, plantilla comentada,
  errores con línea y columna, recarga en caliente (`notify`), rutas normales y modo portable,
  parser de atajos.
- `rukes-platform`: atajo global (`RegisterHotKey`), instancia única, Mica, esquinas redondeadas,
  posicionamiento en el monitor del cursor, robo de foco controlado y devolución del foco al
  ocultar, `ShellExecuteEx` (abrir / como administrador), abrir ubicación, portapapeles,
  extracción de iconos del shell, descubrimiento de apps (menú Inicio, UWP/MSIX, PATH) y recorte
  del working set.
- `rukes-plugins`: plugin de aplicaciones con caché, alias, deduplicado y acciones.
- `rukes-ui`: ventana iced + tiny-skia con campo de búsqueda, resultados con iconos, navegación
  con teclado y ratón, avisos y altura dinámica.
- `rukes-app`: orquestación, comandos internos (salir, recargar, abrir configuración), guardado
  periódico de la frecency.
- ADR 0003 (hilos y canales, sin tokio) y ADR 0004 (estado en JSON versionado).
- `docs/checklists/fase-1.md` y métricas de la fase 1 en `docs/benchmarks.md`.


### Añadido
- Fase 0: workspace de Cargo con los crates `rukes-core`, `rukes-plugins`, `rukes-script`, `rukes-config`, `rukes-platform`, `rukes-ui` y `rukes-app`.
- Logs con `tracing` en consola y archivo rotativo diario (`%LOCALAPPDATA%\Rukes\logs`, 5 archivos).
- CI en GitHub Actions (fmt, clippy, tests, build release).
- Spikes de UI (Direct2D, egui glow/wgpu, iced wgpu/tiny-skia) en `spikes/` con benchmark automático común.

### Decidido
- ADR 0001: motor de UI **iced + tiny-skia** (19 MB WS / 5,6 MB privados visible, 71 ms de arranque). Ver `docs/benchmarks.md`.
- ADR 0002: edición 2024 y lints de workspace.
