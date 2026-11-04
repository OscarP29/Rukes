# Rukes

Launcher de productividad y automatización para Windows 10/11, escrito en Rust: ligero, rápido y con la automatización como ciudadano de primera clase.

> Estado: **Fase 1** (MVP): busca y abre aplicaciones con `Alt+Space`. La especificación completa está en [`Rukes Contexto y Requerimientos.md`](Rukes%20Contexto%20y%20Requerimientos.md).

## Compilar

Requisitos: Rust estable (toolchain MSVC) en Windows 10 22H2+ o Windows 11.

```bash
cargo build --release -p rukes-app
```

El binario queda en `target/release/rukes.exe`. Nivel de log con `RUKES_LOG` (por ejemplo `RUKES_LOG=debug`).

## Uso

1. Ejecuta `rukes.exe`. Queda residente y sin ventana.
2. Pulsa `Alt+Space` y escribe (`chr` → Google Chrome).

| Tecla | Acción |
| --- | --- |
| `Enter` | Abrir |
| `Ctrl+Enter` | Abrir como administrador |
| `Ctrl+Shift+Enter` | Abrir la carpeta contenedora |
| `Ctrl+C` | Copiar la ruta |
| `↑` `↓` / `Tab` / `Ctrl+J` `Ctrl+K` | Moverse por los resultados |
| `Esc` | Ocultar |

- **Configuración:** `%APPDATA%\Rukes\config.toml`, que se crea comentado en el primer arranque y
  se aplica al guardar. También puedes abrirla escribiendo «configuración» en el launcher.
- **Salir:** escribe «salir» en el launcher.
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
| `spikes/` | Prototipos desechables de la fase 0 (workspace aparte) |
| `docs/decisions/` | ADRs |

## Licencia

MIT o Apache-2.0, a elección.
