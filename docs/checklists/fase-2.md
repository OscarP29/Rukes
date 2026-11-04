# Lista de verificación manual: Fase 2 (plugins esenciales)

Leyenda: ✅ verificado · ⏳ pendiente de verificar a mano · ⚠️ verificado con observaciones

Probado el 2026-11-04 con el binario release, Windows 11 Pro 26300 y un solo monitor.

## Casos de uso de aceptación

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| 2. `= 5 km a millas` → 3,1068559612 miles | RF-071 | ✅ | También con `a`/`en` y nombres de unidad en español |
| 3. `> shell:startup` abre la carpeta de inicio | RF-061 | ✅ | El Explorador se abrió en `shell:startup` y Rukes se ocultó |
| 6. `kill chrome` lista los procesos con su RAM y cierra el elegido | RF-051, RF-052 | ⚠️ | Chrome no estaba abierto: se probó la lista con `kill brave` (grupo ×12 con 1,4 GB y cada proceso por RAM) y el flujo completo con `kill ping` (Enter → confirmación → Enter → proceso terminado → lista actualizada) |

## Calculadora y fechas

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| `2^10`, `sqrt(144)`, `20% de 340` | RF-070 | ✅ | Tests |
| Evaluación sin prefijo (`2+2`) y sin falsos positivos (`7zip`, `2026`) | RF-070 | ✅ | Tests |
| `70 F en C`, `100c a f`, `0 C to K` | RF-071 | ✅ | Tests |
| `hora en Tokio` | RF-073 | ✅ | 00:57 del día siguiente con 10:57 en Bogotá |
| `días hasta 25 dic`, `hoy`, `ahora` | RF-074 | ✅ | Tests con fecha inyectada |
| `Enter` copia el resultado y muestra «Copiado» | RF-070 | ⏳ | No se ejecutó para no tocar el portapapeles |

## Shell y sistema

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| `> dir` abre la terminal con el comando (`wt` + `pwsh`) | RF-060 | ⏳ | Línea de comandos cubierta por tests (`-EncodedCommand`, `;` escapado para `wt`) |
| `Ctrl+Enter` ejecuta como administrador | RF-060 | ⏳ | |
| Sin Windows Terminal → consola del shell | RF-060 | ⏳ | |
| `> ms-settings:display` | RF-061 | ⏳ | |
| `apagar` aparece como comando del sistema | RF-062 | ✅ | No se pulsó Enter |
| Enter pide confirmación (`Sí, apagar` / `Cancelar`) | RF-062 | ✅ | Test; `Cancelar` vacía la consulta |
| `bloquear` sin confirmación | RF-062 | ⏳ | |
| `confirm_system = false` ejecuta directamente | RF-062 | ⏳ | |

## Web

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| `github.com` → «Abrir github.com» | RF-090 | ✅ | `notas.txt` o `setup.exe` no se toman por URL |
| `g rust lifetimes` → búsqueda en Google | RF-091 | ✅ | `g` a secas sigue buscando apps |
| Enter abre el navegador | RF-090 | ⏳ | |
| Motores propios en `[plugins.web.engines]` | RF-091 | ⏳ | |

## Ventanas y procesos

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| `win claude` encuentra la ventana (minimizada) | RF-050 | ✅ | |
| Enter la trae al frente y la restaura | RF-050 | ⏳ | |
| `ps` lista todos los procesos por RAM | RF-051 | ⏳ | |
| Procesos del sistema (`csrss`, `lsass`...) sin acción de terminar | RF-051 | ✅ | Test |
| «Terminar … y sus procesos hijos» | RF-052 | ⏳ | Descendientes cubiertos por test |
| CPU % | RF-051 | ⚠️ | Brave en reposo mostró 0,0 % en todos sus procesos; revisar con carga real |

## Generadores

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| `uuid` (v4 y v7) | RF-080 | ✅ | |
| `pass 32`, `lorem 3`, `ts`, `ts 1700000000` | RF-080 | ✅ | Tests |
| `sha256 hola`, `md5`, `sha1`, `sha512`, `crc32` | RF-081 | ✅ | Valores comprobados en tests |
| Hash de un archivo (`sha256 C:\ruta`) | RF-081 | ✅ | Test con archivo temporal |
| `b64`/`b64d`, `url`/`urld`, `html`/`htmld`, `hex`/`unhex` | RF-082 | ✅ | Tests |

## Archivos

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Índice de las carpetas del usuario | RF-040 | ✅ | 70 813 entradas en 2,5–7 s; 2,3 MB |
| Caché en disco: arranque con el índice ya cargado | RF-040 | ✅ | 2–3 ms en cargar 70 813 entradas |
| Watcher: crear, borrar y renombrar se reflejan | RF-040 | ⚠️ | `refresh_dir` cubierto por test (renombrar carpeta con contenido); falta probarlo en vivo |
| Exclusiones (`node_modules`, `.git`, ocultos) | RF-042 | ✅ | Test |
| `d: proyectos` solo carpetas | RF-044 | ✅ | |
| `f:` y filtro `.pdf` | RF-044 | ✅ | Tests |
| Enter abre, `Ctrl+Shift+Enter` ubicación, `Ctrl+C` ruta | RF-043 | ⏳ | |
| `Ctrl+O` abrir con…, `Ctrl+T` terminal aquí | RF-043 | ⏳ | |

## Bandeja e inicio con Windows

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Icono en la bandeja | RF-007 | ⏳ | Se crea sin errores (log y test del icono); falta mirarlo |
| Clic izquierdo muestra la ventana | RF-007 | ⏳ | |
| Menú: Mostrar, Configuración, Recargar, Pausar/Reanudar, Salir | RF-007 | ⏳ | |
| Salir quita el icono (sin icono fantasma) | RF-007 | ⏳ | |
| El icono vuelve si se reinicia el Explorador | RF-007 | ⏳ | |
| `start_with_windows = true` crea la entrada en `Run` y `false` la quita | RF-008 | ⏳ | Probado con una clave de pruebas, no con la real |
