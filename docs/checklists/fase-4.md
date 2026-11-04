# Lista de verificación manual: Fase 4 (temas, previsualización e i18n)

Leyenda: ✅ verificado · ⏳ pendiente de verificar a mano · ⚠️ verificado con observaciones

Probado el 2026-11-04 en modo portable (carpeta temporal con `portable.txt`), Windows 11 Pro
26300, con el sistema en modo oscuro. No se cambió ningún ajuste de Windows (modo claro/oscuro,
contraste alto, escalado de texto, animaciones): esos casos están cubiertos por tests.

## Criterios de aceptación

| Criterio | Estado | Notas |
| --- | --- | --- |
| Los temas incluidos cargan y se recargan en caliente | ✅ | Tokyo Night, Catppuccin Latte, Gruvbox, Nord y Dracula vistos en la app; los 9 cargan en tests. Un tema del usuario (`themes/mio.toml`, `extends = "dracula"`) cambió de acento y selección al guardarlo con la ventana abierta |
| Inglés y español completos | ✅ | `language = "en"` cambia todos los textos en caliente; test: `es.toml` y `en.toml` tienen las mismas claves y cada `tr("…")` del código existe |
| Windows 10 degrada correctamente | ⚠️ | Simulado con `backdrop = "solid"` (el camino que toma Windows 10 al no tener `DWMWA_SYSTEMBACKDROP_TYPE`): fondo opaco del color del tema. No probado en un Windows 10 real |

## Temas y apariencia

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Tema TOML: colores, fuentes, radios, márgenes, opacidad, icono, alto de fila | RF-192 | ✅ | `radius = 14` y acento propio vistos en la app |
| `extends` hereda de otro tema | RF-192 | ✅ | |
| Error en el tema: aviso con línea y se mantiene el anterior | RF-192 | ✅ | `mio.toml:4:10: color no válido «rosa»` |
| Recarga al guardar | RF-193 | ✅ | Carpeta `themes` vigilada |
| Temas incluidos (Catppuccin ×4, Gruvbox, Nord, Tokyo Night, Dracula, claro, sistema) | RF-194 | ✅ | `rukes-light` y `system` incluidos |
| Animaciones 80–150 ms (aparecer, selección, tamaño) | RF-195 | ✅ | 110 / 90 / 100 ms; los fotogramas solo se piden mientras hay una animación |
| `animations = false` y «Efectos de animación» de Windows apagados | RF-195 | ⚠️ | `false` probado en la app; el ajuste de Windows se lee con `SPI_GETCLIENTAREAANIMATION` (no se cambió) |
| Caché de iconos en disco y memoria, carga perezosa | RF-196 | ✅ | `cache/icons/*.rgba` (104 tras unas búsquedas); solo se piden los iconos visibles |
| Ligaduras, Nerd Fonts, emoji en color | RF-197 | ✅ | Emoji en color (🚀, 🧑‍🚀) vistos en la vista previa de un README; las fuentes son configurables (`[theme.font]`) |
| `theme = "system"` sigue el modo claro/oscuro | RF-198 | ⚠️ | Modo oscuro → Tokyo Night en la app; el cambio a claro se comprueba al mostrar la ventana (tests) |

## Vista previa

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Panel a la derecha con `Ctrl+P` y `preview_panel` | RF-017 | ✅ | |
| Texto | RF-017 | ✅ | `Cargo.toml` con ajuste de líneas |
| Markdown | RF-017 | ✅ | Títulos, código, viñetas y emoji |
| Datos del archivo (tipo, tamaño, fecha, ubicación, elementos de carpeta) | RF-017 | ✅ | |
| Imagen (miniatura del shell) | RF-017 | ⏳ | `IShellItemImageFactory` con `SIIGBF_THUMBNAILONLY`; no se abrió una imagen en la prueba |

## Idiomas y accesibilidad

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Español e inglés; por defecto el del sistema | RF-210 | ✅ | |
| Textos en archivos TOML; un idioma nuevo sin recompilar | RF-211 | ✅ | `lang/en.toml` del usuario cambió el texto de búsqueda en caliente; `fr.toml` en tests |
| Escalado de texto de Windows | RF-214 | ⚠️ | Tests (`TextScaleFactor` 150 % → texto, filas y ancho ×1,5); no se cambió el ajuste |
| Contraste alto | RF-214 | ⚠️ | Tests: colores del sistema, fondo opaco sin Mica; no se activó |

## Problemas encontrados y corregidos durante la prueba

- El Markdown largo se cortaba tras la primera línea ajustada: ahora se dibuja como un único
  texto enriquecido.
- Tras la animación de tamaño, iced a veces no redibujaba el último cambio y quedaba una franja
  sin pintar (visible con fondo sólido): se piden 120 ms más de fotogramas al terminar.
- Con fondo sólido al 97 % se transparentaba el texto de detrás: la opacidad sólida por defecto
  pasa a 100 %.
