"""Plugin «{{name}}» de Rukes (protocolo: docs/plugin-protocol.md).

Rukes lo arranca la primera vez que se usa y le habla por stdin/stdout con JSON, un mensaje
por línea. Lo que escribas en stderr va al log de Rukes. Al guardar este archivo o plugin.toml,
Rukes lo recarga solo.
"""

import json
import sys

config = {}


def send(msg):
    sys.stdout.write(json.dumps(msg, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def query(params):
    """Resultados para lo escrito tras la palabra clave."""
    text = params["text"]
    if not text:
        return [{"title": "Escribe algo para probar «{{name}}»", "actions": []}]
    return [
        {
            "id": "mayusculas",
            "title": text.upper(),
            "subtitle": "Enter copia el texto en mayúsculas",
            "data": text.upper(),
        }
    ]


def action(params):
    """Lo que pasa al pulsar Enter: copy, open, notify, query, results, hide o keep_open."""
    return {"copy": params["item"]["data"]}


for line in sys.stdin:
    msg = json.loads(line)
    if "id" not in msg:
        continue  # notificación «cancel»: no se responde
    try:
        if msg["method"] == "initialize":
            config = msg["params"].get("config", {})
            result = {}
        elif msg["method"] == "query":
            result = query(msg["params"])
        else:
            result = action(msg["params"])
        send({"id": msg["id"], "result": result})
    except Exception as e:  # el error se muestra en Rukes sin tumbar el plugin
        send({"id": msg["id"], "error": {"message": str(e)}})
