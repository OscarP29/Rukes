# 0001. Motor de UI: iced con renderer tiny-skia

- Estado: aceptada
- Fecha: 2026-11-04

## Contexto

La sección 2.4 de la especificación pide un *spike* con iced, egui y Direct2D/DirectWrite que muestre
una ventana sin bordes con 8 filas, fondo Mica, esquinas redondeadas y fade-in. Criterios:

- RAM de la ventana visible **< 40 MB**.
- Aparición **< 50 ms**.
- Animaciones fluidas a **60 fps**.

Los tres spikes están en `spikes/` y comparten un arnés (`spikes/common`) que aplica los mismos
efectos DWM y ejecuta el mismo benchmark automático:

1. Primer frame desde la creación del proceso (arranque en frío).
2. 10 ciclos de ocultar y mostrar con `ShowWindow`, midiendo el tiempo hasta el siguiente frame del motor.
3. Memoria tras 3 s oculta.
4. 3 s de animación continua de la selección, midiendo los intervalos entre frames.

Cada variante se ejecutó dos veces. Los resultados en bruto están en `spikes/results/*.json` y la
máquina de referencia en `docs/benchmarks.md`.

## Resultados

Mejor de las dos ejecuciones, con el peor valor entre paréntesis cuando difieren mucho.

| Motor | Exe | Arranque en frío | Mostrar p95 | RAM oculta (WS / privada) | RAM visible (WS / privada) | Animación |
| --- | --- | --- | --- | --- | --- | --- |
| **iced 0.14 + tiny-skia** | 3,4 MB | **71 ms** | 4,3 ms | **19 / 5,5 MB** | **19 / 5,6 MB** | sin vsync: ~160 fps, hueco máx. 10 ms |
| Direct2D + DirectWrite | **0,2 MB** | 373 ms (444) | 6,6 ms | 34 / 15 MB | 34 / 15 MB | 60 fps, hueco máx. 18 ms (95 ms en 1 ejecución) |
| egui 0.35 + glow | 5,7 MB | 218 ms (373) | 5,5 ms (10) | 91 / 88 MB | 99 / 88 MB | 60 fps, hueco máx. 18 ms (30) |
| iced 0.14 + wgpu | 8,6 MB | 443 ms (722) | 4,5 ms (18) | 147 / 128 MB | 147 / 128 MB | 61 fps, hueco máx. 19 ms |
| egui 0.35 + wgpu | 11,2 MB | 486 ms (693) | 5,8 ms | 280 / 320 MB | 280 / 320 MB | 61 fps, hueco máx. 18 ms |

**Sobre las métricas:**

- *WS* es el working set e incluye las DLL compartidas (drivers D3D/GL). *Privada* es la memoria
  propia del proceso.
- «Mostrar» mide desde `ShowWindow` hasta que el motor ejecuta su siguiente frame. En Direct2D se
  mide tras `EndDraw` (incluye el present); en egui e iced se mide en la llamada de dibujo, antes del
  present. Hay que sumar aproximadamente 1 frame de composición de DWM. Aun así, todos quedan muy por
  debajo de 50 ms.

## Evaluación

| Motor | RAM < 40 MB | Aparición < 50 ms | 60 fps | Esfuerzo |
| --- | --- | --- | --- | --- |
| iced + tiny-skia | ✅ | ✅ | ✅ (hay que limitarlo a la tasa del monitor) | Bajo. Widgets de entrada de texto, layout, scroll y foco incluidos |
| Direct2D/DirectWrite | ✅ | ✅ | ✅ | **Alto**. Campo de texto, IME, selección, scroll, layout y accesibilidad hechos a mano |
| egui + glow | ❌ | ✅ | ✅ | Bajo, pero winit reaplica los estilos de ventana y DWM pinta botones de título fantasma al extender el marco (requiere subclassing) |
| iced + wgpu | ❌ | ✅ | ✅ | Bajo |
| egui + wgpu | ❌ | ✅ | ✅ | Bajo. Además, egui 0.36+ exige Rust 1.95 |

Revisión visual con capturas de d2d, egui-glow, iced-wgpu e iced-skia (egui-wgpu no se capturó):
fondo translúcido y esquinas redondeadas correctos en todas; egui muestra además los botones de
título fantasma. El fade-in se implementa como opacidad del contenido. El fondo Mica aparece de
inmediato porque lo pinta DWM.

## Decisión

**iced 0.14 con el renderer `tiny-skia`** (render por CPU con softbuffer) es el motor de UI de Rukes.

- Es la única opción que cumple los tres criterios y además reduce el esfuerzo. La RAM privada
  (5,5 MB) y el arranque (71 ms) superan incluso a Direct2D, porque no carga drivers de GPU.
- Los efectos de Windows 11 (Mica, esquinas, sombra) los aplica DWM sobre el HWND, así que no
  dependen del renderer.
- **Plan B:** Direct2D/DirectWrite. El spike funcional queda en `spikes/d2d` como referencia.

## Consecuencias

- `rukes-ui` depende de `iced` con `default-features = false` y las features `tiny-skia`,
  `canvas`, `advanced` y `thread-pool`. Nunca `wgpu`.
- El resto del proyecto no ve tipos de iced: `rukes-ui` expone su propia API (ventana, lista de
  resultados, eventos de teclado). Así se puede cambiar a Direct2D si hiciera falta.
- **Tareas para la fase 1 derivadas del spike:**
  - Limitar los frames de animación a la tasa de refresco del monitor. tiny-skia no tiene vsync y
    renderiza a ~160 fps, con un coste de CPU innecesario.
  - Medir el uso de CPU durante las animaciones y el coste de raster a 4K/200 %.
  - Medir la latencia hotkey → píxel con el present incluido (ETW o captura de alta velocidad).
  - Verificar la calidad del texto (cosmic-text en escala de grises frente a ClearType) y el
    soporte de Nerd Fonts y emojis a color (RF-197).
  - Verificar IME (japonés/chino) en el campo de búsqueda.
- **Riesgo:** iced es 0.x y cambia de API entre versiones menores. Hay que fijar la versión exacta y
  aislarla en `rukes-ui`.
- **Riesgo:** con tiny-skia el coste de render escala con los píxeles. Si a 4K se superan ~8 ms por
  frame, se reevalúa (renderizado parcial, caché de capas o plan B).
