# Lista de verificación manual: Fase 5 (portapapeles, snippets y notas)

Leyenda: ✅ verificado · ⏳ pendiente de verificar a mano · ⚠️ verificado con observaciones ·
➖ fuera de alcance

Probado el 2026-11-04 en modo portable (carpeta temporal con `portable.txt`), Windows 11 Pro
26300. El pegado se probó sobre una ventana propia de prueba (un cuadro de texto de WinForms),
no sobre aplicaciones del usuario. El script de prueba guardaba en memoria el portapapeles del
usuario y lo devolvía al terminar, con la instancia de prueba ya cerrada para que no lo
registrara.

## Criterios de aceptación

| Criterio | Estado | Notas |
| --- | --- | --- |
| Caso de uso 8: «Escribo `clip`: busco en el historial del portapapeles y pego» | ✅ | `clip` (con o sin espacio) lista el historial arriba; `clip uno` + Enter pegó «rukes prueba uno» en la ventana anterior, que recuperó el foco |
| No se registra el contenido marcado como sensible | ✅ | Copias con `ExcludeClipboardContentFromMonitorProcessing` y con `CanIncludeInClipboardHistory = 0`: `clip secreto` no muestra nada y el historial descifrado no las contiene |

## Historial del portapapeles (5.10)

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Registra el texto copiado, con búsqueda | RF-100 | ✅ | Ventana de solo mensajes con `AddClipboardFormatListener`; origen (`powershell.exe`), antigüedad y tamaño en el subtítulo; contenido completo en la vista previa (`Ctrl+P`) |
| Imágenes en el historial | RF-100 | ➖ | Opcional en la especificación; solo texto en esta fase |
| Fijar (`Ctrl+S`): no caduca ni cuenta para el límite | RF-101 | ✅ | Tests |
| Límite y caducidad configurables | RF-101 | ✅ | `max_items`, `expire_days`; tests |
| Lista de exclusión de apps | RF-102 | ✅ | `exclude` (KeePass, KeePassXC, 1Password, Bitwarden…); tests |
| Formatos de Windows de contenido sensible | RF-102 | ✅ | También `Clipboard Viewer Ignore` |
| Enter pega en la ventana anterior (o solo copia con `paste = false`) | RF-103 | ✅ | Suelta los modificadores que siga pulsando el usuario antes de `Ctrl+V` |
| Historial cifrado con DPAPI | RF-104 | ✅ | `%LOCALAPPDATA%\Rukes\clipboard\history.bin`: sin texto en claro; se descifra con la cuenta del usuario |
| Desactivar el plugin | RF-104 | ✅ | `enabled = false` deja de escuchar el portapapeles (se suelta el vigilante) |

## Snippets (5.11)

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Archivos TOML y Markdown con nombre, palabra clave y contenido | RF-110 | ✅ | La primera vez se crea `snippets/ejemplos.toml`; la carpeta se vigila (`.toml`, `.md`) |
| `{fecha}`, `{hora}`, `{portapapeles}`, `{cursor}` | RF-111 | ✅ | `hoy` pegó `04/11/2026`; `{cursor}` deja el cursor en su sitio (tests) |
| Campos rellenables | RF-111 | ✅ | `reunion` muestra «Escribe «reunion Persona; Día»…»; `reunion Ana; martes` pegó «Hola Ana, ¿te va bien vernos el martes?» |
| Insertar en la ventana anterior o copiar | RF-112 | ✅ | Tras insertar, el portapapeles vuelve a tener lo de antes (comprobado) y el snippet no entra en el historial |
| Expansión automática global | RF-113 | ➖ | Prioridad C: no se implementa en esta fase |

## Notas y tareas (5.12)

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| `note <texto>` / `nota <texto>` añade una nota con fecha | RF-120 | ✅ | `- 2026-11-04 15:43 — probando las notas de rukes`; `note` solo lista las últimas |
| `todo <texto>` añade; `todo` lista y Enter completa | RF-121 | ✅ | `- [x] llamar a Ana ✅ 2026-11-04`; si el archivo cambió por fuera, la tarea se busca por su texto |
| Markdown plano, compatible con Obsidian | RF-122 | ✅ | Rutas configurables (`notes_file`, `todo_file`) |
| `remind 15m llamar a Ana` / `recordar …` con notificación | RF-123 | ✅ | El de 5 s avisó con una notificación de la bandeja; `Ctrl+D` cancela; los vencidos con Rukes cerrado avisan al arrancar (tests) |

## Problemas encontrados y corregidos durante la prueba

- `clip`, `todo` o `recordar` sin espacio buscaban solo en apps y archivos. Ahora, si la consulta
  es justo la palabra clave de uno de estos plugins, su lista aparece arriba sin quitar la
  búsqueda global. Otras palabras clave (`win`, `kill`, `g`) siguen como antes.
- Un campo o variable dejaba la llave de cierre en el texto expandido (`Ana}`).
- La caducidad del historial usaba la hora de la copia en lugar de la actual.
