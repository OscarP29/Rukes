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

## Fase 1: MVP (2026-11-04)

Binario `rukes.exe` release: **6,0 MB**. Índice: 454 apps (accesos directos, UWP y PATH).

| Métrica | Objetivo | Medido | Estado |
| --- | --- | --- | --- |
| RNF-001 RAM en reposo (oculta) | ≤ 20 MB working set | **2,3 MB WS** tras recorte · 21,8 MB privada | ✅ (ver nota) |
| RNF-003 Atajo → ventana con foco | ≤ 50 ms p95 | 11–15 ms hasta que la UI procesa «mostrar» (5 ciclos), + ~1 frame | ✅ |
| RNF-004 Pulsación → resultados (10 000 elementos) | ≤ 16 ms | 4,7 ms (peor consulta del benchmark) | ✅ |
| RNF-005 Arranque hasta «listo» | ≤ 300 ms | 28–43 ms (atajo registrado, caché cargada); ventana creada a ~150 ms | ✅ |
| Reindexado de apps (segundo plano) | — | 1,0–1,2 s | — |

**Notas:**

- **RNF-001:** al ocultar la ventana, tras 1,5 s se llama a `EmptyWorkingSet`, y el working set
  baja de ~56 MB a ~2 MB. La memoria privada confirmada (~22 MB) no baja: viene sobre todo de las
  DLL del shell que carga la extracción de iconos y de la base de fuentes de cosmic-text. Las
  páginas vuelven con fallos suaves y la latencia de aparición sigue en 11–15 ms. Reducir la
  privada queda como tarea: extraer iconos en un proceso auxiliar o con caché en disco (RF-196).
- **RNF-003** se mide desde la llamada al atajo hasta que la UI procesa el evento. El primer frame
  llega en el siguiente ciclo de render.
- **RNF-004, 100 000 elementos:** 60 ms con `chr`. Antes del índice de archivos (fase 2) hace falta
  matcher paralelo o prefiltrado.

Benchmark de búsqueda:

```bash
cargo bench -p rukes-core --bench search
```
