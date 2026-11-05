# Protocolo de plugins externos (v1)

Un plugin externo es un programa cualquiera (Python, Rust, Node, PowerShell…) que Rukes arranca
como proceso aparte y con el que habla por **stdin/stdout** con mensajes **JSON, uno por línea**
(NDJSON, UTF-8). Así un plugin que se cuelga o se cae no afecta al launcher.

Ejemplos completos: [`plugins-examples/cuenta`](../plugins-examples/cuenta) (Python) y
[`plugins-examples/color`](../plugins-examples/color) (Rust).

## Instalación

Lo más rápido: escribe `nuevo plugin Mi plugin` en Rukes y elige el lenguaje (Python, Node.js
o PowerShell). Rukes crea la carpeta con un `plugin.toml` y un programa de ejemplo que ya
funciona y lo abre en el editor.


Cada plugin vive en su carpeta, dentro de la carpeta de plugins:

```
%APPDATA%\Rukes\plugins\
  cuenta\
    plugin.toml
    main.py
```

La carpeta se vigila: al copiar o borrar una carpeta de plugin, o al guardar su `plugin.toml`,
Rukes recarga los plugins sin reiniciar. Al instalar uno nuevo, una notificación muestra su nombre,
su versión y sus permisos. Escribe `plugins` en el launcher para ver los instalados, su estado,
sus permisos y los errores del `plugin.toml`.

La carpeta se cambia con `[plugins.external] dir` en `config.toml` (`enabled = false` desactiva
todos los plugins externos).

## `plugin.toml`

```toml
[plugin]
name = "Cuenta"                # nombre visible (obligatorio)
version = "1.0.0"              # obligatorio
executable = "python"          # obligatorio: ruta relativa a la carpeta del plugin o comando del PATH
args = ["-u", "main.py"]       # argumentos
keywords = ["cuenta", "count"] # palabras clave: "cuenta hola" le envía "hola"
prefix = "%"                   # prefijo opcional: "%hola"
global = false                 # true = también participa en las búsquedas sin palabra clave
priority = 0.0                 # -1..1, se suma a la puntuación
description = "…"
author = "…"
icon = "calc"                  # icono por defecto: archivo (con extensión) o nombre de glifo
enabled = true                 # false = instalado pero apagado

[permissions]                  # informativos en v1.0: se muestran al instalar y en `plugins`
network = false
clipboard = true
filesystem = "read"

[config]                       # libre: el plugin lo recibe en `initialize`
palabras_por_minuto = 200

[help]                         # opcional: lo que muestra «help cuenta» / «info cuenta»
summary = "Cuenta palabras, caracteres y tiempo de lectura"
tips = ["Enter copia el resultado"]

[[help.usage]]                 # un ejemplo por bloque; Enter en la ayuda lo escribe
example = "cuenta hola mundo"
description = "Palabras y caracteres del texto"
```

Hace falta al menos `keywords`, `prefix` o `global = true`. Un campo desconocido en `[plugin]` es
un error (para detectar erratas). El id del plugin es `ext:<nombre de la carpeta>`.

### Ayuda (`help <comando>`)

`help`, `info` o `ayuda` seguidos de una palabra clave, el prefijo o el nombre del plugin
muestran su ayuda: el resumen, un resultado por ejemplo (Enter lo escribe en el launcher para
probarlo) y los consejos. Sin sección `[help]` se usan la `description` y las palabras clave.
Los plugins integrados la toman de los textos de la interfaz (`[usage.<id>]` en
`crates/rukes-core/lang`) o implementan `Plugin::help`.

## Ciclo de vida

- El proceso se arranca **la primera vez que se usa**, con la carpeta del plugin como directorio
  de trabajo, sin ventana, y con las variables `RUKES_PROTOCOL=1`, `PYTHONUNBUFFERED=1` y
  `PYTHONIOENCODING=utf-8`.
- Lo primero que recibe es `initialize`, que debe responder en **5 s**.
- **stderr** va al log de Rukes (`%LOCALAPPDATA%\Rukes\logs`): úsalo para depurar.
- El plugin **debe terminar cuando se cierra stdin** (Rukes se cierra o recarga los plugins).
  Rukes, además, termina el proceso.
- Si el proceso muere, se vuelve a arrancar en la siguiente consulta.

## Tiempos máximos y fallos

| Llamada | Tiempo máximo |
| --- | --- |
| `initialize` | 5 s |
| `query` | 2 s |
| `action` | 10 s |

Agotar el tiempo, morir o no poder arrancar cuenta como **fallo**: el proceso se termina y se
arrancará de nuevo en el siguiente uso. Con **3 fallos seguidos** el plugin queda **desactivado**
hasta que se recargue (guardar su `plugin.toml`) o se reinicie desde la lista `plugins` (Enter).
Una respuesta correcta pone el contador a cero. Un `error` del plugin (ver abajo) no cuenta como
fallo.

Mientras espera a un plugin, Rukes sigue mostrando los resultados de los demás: cada plugin
entrega por su cuenta y una consulta nueva (el usuario sigue escribiendo) abandona la anterior
al instante.

## Mensajes

Peticiones de Rukes (con `id`, esperan respuesta):

```json
{"id": 1, "method": "initialize", "params": {"protocol": 1, "config": {"palabras_por_minuto": 200}, "locale": "es", "plugin_dir": "C:\\…\\plugins\\cuenta"}}
{"id": 2, "method": "query", "params": {"query_id": 17, "text": "hola mundo", "raw": "cuenta hola mundo", "targeted": true}}
{"id": 3, "method": "action", "params": {"item": {"id": "palabras", "data": "2"}, "action": "open"}}
```

Notificación de Rukes (sin `id`, no se responde):

```json
{"method": "cancel", "params": {"query_id": 17}}
```

Se envía cuando la consulta `query_id` ya no interesa (el usuario siguió escribiendo). Un plugin
lento puede usarla para dejar de trabajar; su respuesta, si llega, se ignora.

Respuestas del plugin (con el mismo `id`):

```json
{"id": 1, "result": {"name": "cuenta"}}
{"id": 2, "result": {"items": [ … ]}}
{"id": 2, "error": {"message": "texto del error"}}
```

Notificaciones del plugin (sin `id`):

```json
{"method": "results", "params": {"query_id": 17, "items": [ … ]}}
{"method": "log", "params": {"message": "texto para el log"}}
```

`results` entrega resultados **parciales** de una consulta antes de su respuesta final (por
ejemplo, los primeros de una búsqueda lenta). La respuesta final añade los suyos.

### `query`

- `text`: texto sin la palabra clave ni el prefijo.
- `raw`: lo que escribió el usuario.
- `targeted`: `true` si se usó la palabra clave o el prefijo; `false` en búsquedas globales
  (solo con `global = true`). En las globales, `text` nunca está vacío.

El resultado es `{"items": [...]}` o directamente la lista. Cada resultado:

```json
{
  "id": "palabras",
  "title": "2 palabras",
  "subtitle": "texto secundario",
  "icon": "icono.png",
  "score": 0.9,
  "preview": "C:\\ruta\\imagen.png",
  "detail": "texto para el panel de previsualización",
  "data": {"lo que": "quieras"},
  "actions": ["open", {"id": "copiar", "label": "Copiar"}]
}
```

Solo `title` es obligatorio.

| Campo | Significado |
| --- | --- |
| `id` | Id estable del resultado (para la frecency). Por defecto, su posición. |
| `icon` | Con extensión: archivo, relativo a la carpeta del plugin. Sin extensión: nombre de glifo. Por defecto, el `icon` del manifiesto. |
| `score` | 0..1, cuánto encaja; por defecto, el orden de la lista. |
| `preview` | Archivo o carpeta para el panel de previsualización (`Ctrl+P`). |
| `detail` | Texto para el panel cuando no hay archivo. |
| `data` | Lo que quieras: vuelve en `action`. |
| `actions` | La primera es la de Enter. Una cadena es id y texto a la vez (`open`, `copy_path` y `open_location` se traducen). Por defecto, `["open"]`. |

### `action`

`params.item` es `{"id", "data"}` del resultado y `params.action` el id de la acción. El
resultado dice qué hacer después:

| Resultado | Efecto |
| --- | --- |
| `"hide"` (o cualquier otra cosa) | Ocultar el launcher |
| `"keep_open"` | Dejarlo abierto |
| `{"query": "texto"}` | Cambiar la búsqueda |
| `{"notify": "texto"}` | Mostrar un aviso breve y dejarlo abierto |
| `{"copy": "texto"}` | Copiar al portapapeles y avisar |
| `{"open": "ruta o URL"}` | Abrir con el programa predeterminado y ocultar |
| `{"results": [ … ]}` | Sustituir la lista por estos resultados (submenús) |

## Comandos de otros plugins

Algunos plugins integrados ofrecen comandos sin interfaz para que otros los usen:

| Plugin | Comando | Argumentos → resultado |
| --- | --- | --- |
| `notes` | `add` | `{text}` → `{text, file}` |
| `notes` | `list` | `{limit: 10}` → `["texto", …]` |
| `notes` | `delete` | `{text}` (como lo da `list`) → `true` si existía |
| `todo` | `add` | `{text}` → `{text, file}` |
| `todo` | `list` | → `[{line, text}, …]` |
| `todo` | `complete` | `{text}` → `true` si existía |
| `remind` | `add` | `{when: "15m" \| "1h30m" \| "14:30"}`, `{seconds}` o `{at}` (Unix) y `{text}` → `{id, due, text}` |
| `remind` | `list` | → `[{id, due, text}, …]` |
| `remind` | `cancel` | `{id}` → `true` si existía |

Hoy se usan desde los plugins integrados (`Engine::call`) y desde los scripts Rhai
(`plugin_call("remind", "add", #{ when: "15m", text: "té" })`, o los atajos `note_add`,
`todo_add` y `remind`; ver [`scripts-examples/telegram.rhai`](../scripts-examples/telegram.rhai)).
Los plugins externos aún no pueden llamarlos (el protocolo v1 solo va de Rukes al plugin); mientras tanto, un
plugin externo puede escribir directamente en los Markdown de notas y tareas.

## Plugin mínimo en Python

```python
import json, sys

for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue  # cancel
    if msg["method"] == "initialize":
        result = {}
    elif msg["method"] == "query":
        text = msg["params"]["text"]
        result = [{"title": text.upper(), "data": text.upper()}]
    else:  # action
        result = {"copy": msg["params"]["item"]["data"]}
    print(json.dumps({"id": msg["id"], "result": result}), flush=True)
```
