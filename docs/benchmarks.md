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

## Fase 2: plugins esenciales (2026-11-04)

Binario `rukes.exe` release: **8,5 MB** (6,0 MB en la fase 1). Índices: 452 apps y 70 813
archivos de las carpetas del usuario.

| Métrica | Objetivo | Medido | Estado |
| --- | --- | --- | --- |
| RNF-001 RAM en reposo (oculta) | ≤ 20 MB working set | **2,5 MB WS** tras recorte · 23,3 MB privada | ✅ |
| RNF-004 Búsqueda de archivos, 500 000 entradas | ≤ 16 ms | p95 9–14 ms según la consulta (8 hilos) | ✅ |
| RNF-005 Arranque hasta «listo» | ≤ 300 ms | 62–80 ms en caliente (~22 ms dentro de `main`) | ✅ (ver nota) |
| RNF-007 Índice de 500 000 archivos | ≤ 60 MB extra | **25,9 MB** extra (24,5 MB contabilizados) | ✅ |
| Índice real (70 813 entradas) | — | 2,3 MB · recorrido 2,5–7 s · caché 2–3 ms | — |
| Caché de 500 000 entradas | — | 14,7 MB en disco · guardar 15 ms · cargar 17 ms | — |

**Notas:**

- **RNF-005:** en la fase 1 eran 28–43 ms. Lo que pasa dentro de `main` sigue en ~22 ms; el resto
  es la carga del proceso (binario más grande y más DLL importadas: energía, ToolHelp, etc.).
  El icono de la bandeja ya no bloquea el arranque.
- **RNF-007:** cada entrada ocupa 12 bytes más su nombre en un único `String`; las rutas se
  reconstruyen subiendo por los padres. La búsqueda descarta primero por subsecuencia ASCII y
  solo puntúa con `nucleo` lo que pasa, en bloques paralelos con un top-K por hilo.
- Durante un recorrido completo conviven el índice viejo y el nuevo (memoria ×2 temporal).

Reproducir:

```bash
cargo run --release -p rukes-plugins --example files_bench
```

## Fase 3: sistema de scripts (2026-11-04)

Binario `rukes.exe` release: **12,2 MB** (8,5 MB en la fase 2; Rhai añade ~3,7 MB).

| Métrica | Objetivo | Medido | Estado |
| --- | --- | --- | --- |
| RNF-001 RAM en reposo (oculta) | ≤ 20 MB working set | **1,9 MB WS** tras recorte · 26 MB privada | ✅ |
| RNF-005 Arranque hasta «listo» | ≤ 300 ms | 61–66 ms | ✅ |
| Script colgado con `timeout 2s` | se cancela sin afectar al launcher | terminado a los 2 019 ms; el launcher siguió respondiendo | ✅ |
| Script PowerShell (`gh issue Mi bug`) | — | 597–705 ms de principio a fin (arranque de `pwsh`) | — |

**Notas:**

- Memoria privada: 26 MB recién arrancado (23,3 MB en la fase 2). Tras ejecutar scripts y mostrar
  listas largas llegó a 40 MB; el working set vuelve a ~2–7 MB al ocultar.
- Las medidas se tomaron con una copia portable (`portable.txt`) para no tocar la configuración
  del usuario.

## Fase 5: portapapeles, snippets y notas (2026-11-04)

Binario `rukes.exe` release: **12 984 320 bytes**.

| Métrica | Objetivo | Medido | Estado |
| --- | --- | --- | --- |
| RNF-001 RAM en reposo (oculta) | ≤ 20 MB working set | **1,6 MB WS** tras recorte · 15,1 MB privada | ✅ |
| RNF-005 Arranque hasta «listo» | ≤ 300 ms | 124–168 ms (3 arranques) | ✅ |

**Notas:**

- Hilos nuevos en reposo: escucha del portapapeles (bloqueada en `GetMessageW`), guardado del
  historial y recordatorios (bloqueados en un canal; los recordatorios despiertan como mucho
  una vez por minuto), y el vigilante de la carpeta `snippets`.
- El historial se guarda cifrado agrupando los cambios (800 ms) y al salir.
- Medido con una copia portable (`portable.txt`) para no tocar la configuración del usuario.

## Fase 4: temas, previsualización e i18n (2026-11-04)

Binario `rukes.exe` release: **12,6 MB** (12,2 MB en la fase 3).

| Métrica | Objetivo | Medido | Estado |
| --- | --- | --- | --- |
| RNF-001 RAM en reposo (oculta) | ≤ 20 MB working set | **1,8 MB WS** tras recorte · 21,8 MB privada | ✅ |
| RNF-005 Arranque hasta «listo» | ≤ 300 ms | 106–108 ms (219 ms con el binario recién copiado) | ✅ |
| Carga del tema | — | ~3 ms | — |
| Animaciones | 80–150 ms | aparecer 110 ms · selección 90 ms · tamaño 100 ms | ✅ |

**Notas:**

- El arranque sube respecto a la fase 3 (61–66 ms). El tema solo suma ~3 ms; el resto se
  reparte entre configuración, plugins, bandeja y los dos vigilantes de carpetas nuevos
  (`themes`, `lang`). No se ha aislado la causa; sigue muy por debajo del objetivo.
- En reposo no se piden fotogramas: la suscripción a `window::frames()` solo existe mientras
  hay una animación.
- Medido con una copia portable (`portable.txt`) para no tocar la configuración del usuario.

## Fase 8: release 1.0 (2026-11-04)

Binario `rukes.exe` release: **14,5 MB** (15 188 992 bytes; 12,4 MB en la fase 5). Lo que más
suma: el CRT enlazado estáticamente (RNF-031), las API de Windows nuevas (audio, WMI, radios,
Everything), el esquema de la configuración (schemars) y el icono de 16 a 256 px. Queda a
0,5 MB del límite de 15 MB (RNF-008); CI falla si se supera. Margen posible si hiciera falta:
`opt-level = "z"` en crates fríos o quitar los tamaños grandes del icono.

### Panel de ajustes, leyenda y botones de acción

El panel visual (listas, interruptores, deslizadores, `toml_edit`) subió el binario a 14,8 MB.
Con Rhai compilado con `opt-level = "s"` (`[profile.release.package.rhai]`; los scripts no
están en el camino de la búsqueda) queda en **14,55 MB** (15 256 576 bytes), 0,45 MB
por debajo del límite.

### Ayuda de comandos y plugin HTTP

**14,74 MB** (15 454 208 bytes): +0,19 MB por los textos de ayuda y el plugin `http`.
Quedan 0,26 MB hasta el límite de 15 MB.
