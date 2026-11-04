# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/); versionado [SemVer](https://semver.org/lang/es/).

## [Unreleased]

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
