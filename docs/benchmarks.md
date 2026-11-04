# Benchmarks

## Máquina de referencia

| | |
| --- | --- |
| CPU | AMD Ryzen 5 5500U (6 núcleos / 12 hilos) |
| GPU | AMD Radeon Graphics (integrada) |
| RAM | 12 GB (11,3 GB utilizables) |
| SO | Windows 11 Pro, build 26300 |
| Pantalla | 1920×1080 a 100 % (96 DPI), 60 Hz |
| Toolchain | Rust 1.93.1 MSVC, perfil release (LTO fat, codegen-units 1) |

Portátil con batería y ahorro de energía activo durante las mediciones de la fase 0. Las cifras
absolutas pueden variar; sirven para comparar entre fases.

## Fase 0: spikes de UI (2026-11-04)

Ver [ADR 0001](decisions/0001-motor-ui.md). Datos en bruto en `spikes/results/`.

| Motor | Arranque en frío | Mostrar p95 | RAM visible (WS / privada) | Animación |
| --- | --- | --- | --- | --- |
| iced + tiny-skia | 71 ms | 4,3 ms | 19 / 5,6 MB | ~160 fps sin vsync |
| Direct2D | 373 ms | 6,6 ms | 34 / 15 MB | 60 fps |
| egui + glow | 218 ms | 5,5 ms | 99 / 88 MB | 60 fps |
| iced + wgpu | 443 ms | 4,5 ms | 147 / 128 MB | 61 fps |
| egui + wgpu | 486 ms | 5,8 ms | 280 / 320 MB | 61 fps |

Reproducir:

```bash
cd spikes
cargo build --release -p spike-d2d
SPIKE_NAME=d2d ./target/release/spike-d2d.exe
```
