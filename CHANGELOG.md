# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/); versionado [SemVer](https://semver.org/lang/es/).

## [Unreleased]

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
