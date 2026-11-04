# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/); versionado [SemVer](https://semver.org/lang/es/).

## [Unreleased]

### Añadido
- Fase 0: workspace de Cargo con los crates `rukes-core`, `rukes-plugins`, `rukes-script`, `rukes-config`, `rukes-platform`, `rukes-ui` y `rukes-app`.
- Logs con `tracing` en consola y archivo rotativo diario (`%LOCALAPPDATA%\Rukes\logs`, 5 archivos).
- CI en GitHub Actions (fmt, clippy, tests, build release).
- Spikes de UI (Direct2D, egui glow/wgpu, iced wgpu/tiny-skia) en `spikes/` con benchmark automático común.

### Decidido
- ADR 0001: motor de UI **iced + tiny-skia** (19 MB WS / 5,6 MB privados visible, 71 ms de arranque). Ver `docs/benchmarks.md`.
- ADR 0002: edición 2024 y lints de workspace.
