# Lista de verificación manual: Fase 3 (sistema de scripts)

Leyenda: ✅ verificado · ⏳ pendiente de verificar a mano · ⚠️ verificado con observaciones

Probado el 2026-11-04 con el binario release en modo portable (carpeta temporal con
`portable.txt`), Windows 11 Pro 26300.

## Casos de uso de aceptación

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| 4. `new script` crea una plantilla, la abre en el editor y aparece en el launcher | RF-135, RF-130 | ✅ | `new script Prueba` → 5 plantillas; Enter creó `prueba.ps1` y lo abrió en el Bloc de notas; `prueba Ana` lo ejecutó |
| 9. `gh issue Mi bug` ejecuta el script con `Mi bug` como argumento | RF-133 | ✅ | Script de prueba que muestra el título en vez de llamar a `gh`; gana al buscador web `gh` |
| Un script colgado se cancela por timeout sin afectar al launcher | RF-137 | ✅ | `Start-Sleep 120` con `timeout 2s`: terminado a los 2 019 ms, sin procesos huérfanos |

## Scripts

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Scripts nuevos aparecen sin reiniciar | RF-130 | ✅ | 2 → 4 scripts al copiarlos con Rukes abierto |
| Ejemplos copiados al crear la carpeta | RF-130 | ✅ | `hola.ps1`, `mi-ip.rhai` |
| `.ps1`, `.cmd` | RF-131 | ✅ | App y tests |
| `.py`, `.js`, `.lua`, intérprete propio | RF-131 | ⏳ | Resolución de intérpretes cubierta por tests |
| `.rhai` | RF-131, RF-139 | ✅ | Tests (API, límites, sandbox); no se ejecutó `mi-ip` para no tocar el portapapeles |
| Cabecera con todas las claves y estilos de comentario | RF-132 | ✅ | Tests |
| Argumentos con tipo, valores por defecto y opciones | RF-133 | ✅ | Tests; `Escribe: title (text)` si falta |
| Modo launcher | RF-134 | ✅ | Salida reemplaza a «Ejecutando…» |
| Modo notificación | RF-134 | ✅ | Toast con la última línea (`Hola desde Prueba: Ana` tras corregir la plantilla) |
| Modos silencioso, portapapeles y terminal | RF-134 | ⏳ | Lógica cubierta por tests; no se probó para no tocar el portapapeles ni abrir consolas |
| `Ctrl+Enter` como administrador | RF-134 | ⏳ | Requiere UAC |
| `Ctrl+E` editar y `Ctrl+D` borrar (papelera, con confirmación) | RF-136 | ⚠️ | Confirmación por test; no se borró nada de verdad |
| Cancelar desde «Ejecutando…» o `logs` | RF-137 | ✅ | Tests (job object) |
| `logs` lista ejecuciones y muestra la salida | RF-138 | ✅ | |
| API Rhai: ventanas, HTTP real, `open` | RF-139 | ⏳ | HTTP probado contra un servidor local en tests |
| *Script filter* JSON | RF-140 | ✅ | Test con `.cmd`; ejemplo `proyectos.py` no probado |
| `RUKES_ACTIVE_WINDOW`, `RUKES_ARG_*`, `RUKES_QUERY` | RF-141 | ✅ | Visto en la salida del script de prueba |
| `RUKES_SELECTED_FILE` desde el Explorador | RF-141 | ⏳ | |
