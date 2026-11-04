# Lista de verificación manual — Fase 1 (MVP)

Leyenda: ✅ verificado · ⏳ pendiente de verificar a mano · ⚠️ verificado con observaciones

## Invocación y ventana

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| `Alt+Space` muestra la ventana con foco desde otra app en primer plano | RF-001 | ✅ | Probado con pulsación inyectada |
| `Alt+Space` con la ventana visible la oculta | RF-001 | ✅ | |
| Atajo cambiado en `config.toml` se aplica sin reiniciar | RF-001, RF-201 | ⏳ | |
| Atajo en uso por otra app → aviso en la ventana | RF-001 | ⏳ | |
| Aparece centrada (algo por encima) en el monitor del cursor | RF-002 | ✅ | Un solo monitor |
| `position = "top_center"` | RF-002 | ⏳ | |
| Posición estable aunque cambie el número de resultados | RF-002 | ✅ | |
| `Esc` oculta y devuelve el foco a la app anterior | RF-003 | ✅ | |
| Perder el foco (clic fuera) oculta | RF-003 | ⏳ | |
| Tras lanzar una app se oculta | RF-003 | ⏳ | |
| Campo con foco al aparecer; `on_show = select_all` selecciona el texto | RF-004 | ✅ | |
| `on_show = clear` / `keep` | RF-004 | ⏳ | |
| Dos monitores con DPI distinto (100 % / 150 %) | RF-005 | ⏳ | |
| Segunda ejecución de `rukes.exe` muestra la ventana de la primera | RF-006 | ✅ | |

## Búsqueda y resultados

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| `chr` → Google Chrome primero (caso de uso 1) | RF-010 | ✅ | Con iconos reales |
| Frecency: lo que más abro sube | RF-011 | ⏳ | Cubierto por tests; falta uso real |
| Flechas, `Tab`/`Shift+Tab`, `Ctrl+J/K`, `RePág/AvPág` | RF-012 | ⚠️ | Flechas ✅; el resto pendiente |
| `Enter` abre la app | RF-012 | ⏳ | No se ejecutó para no abrir apps en el equipo |
| `Ctrl+Enter` abre como administrador (UAC) | RF-013 | ⏳ | |
| `Ctrl+Shift+Enter` abre la carpeta con el archivo seleccionado | RF-013 | ⏳ | |
| `Ctrl+C` copia la ruta y muestra «Copiado» | RF-013 | ⏳ | |
| Icono, título y subtítulo por resultado | RF-014 | ✅ | |
| Clic en un resultado lo ejecuta | RF-014 | ⏳ | |
| Rueda del ratón desplaza la lista | RF-020 | ⏳ | |
| `max_results` en caliente | RF-020 | ✅ | |

## Aplicaciones

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Accesos directos del menú Inicio | RF-030 | ✅ | |
| Apps de la Tienda (UWP/MSIX) | RF-030 | ⏳ | Indexadas; falta abrir una |
| Ejecutables del PATH (sin System32 ni `usr\bin` de Git) | RF-030 | ✅ | |
| Alias de `[plugins.apps.aliases]` | RF-031 | ⏳ | Cubierto por tests |
| Comandos internos: «Salir de Rukes», «Recargar», «Abrir configuración» | — | ⏳ | |

## Apariencia y configuración

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Mica en Windows 11 | RF-190 | ✅ | |
| Windows 10: fondo sólido | RF-190 | ⏳ | Sin equipo con Windows 10 |
| Esquinas redondeadas y sombra | RF-191 | ✅ | |
| Sin `config.toml` → se crea la plantilla comentada | RF-200 | ✅ | |
| Error de sintaxis → aviso con archivo:línea:columna y se mantiene la config anterior | RF-201 | ✅ | |
| `enabled = false` en `[plugins.apps]` quita las apps | RF-205 | ⏳ | |
| Juego a pantalla completa en primer plano + atajo | — | ⏳ | |
