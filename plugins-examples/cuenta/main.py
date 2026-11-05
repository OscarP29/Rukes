"""Plugin de ejemplo de Rukes en Python (protocolo: docs/plugin-protocol.md).

Lee mensajes JSON, uno por línea, de stdin y responde por stdout. Lo que escribas en stderr
va al log de Rukes. Termina cuando se cierra stdin.
"""

import json
import sys
import time

config = {}


def send(msg):
    sys.stdout.write(json.dumps(msg, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def query(params):
    text = params["text"]
    if text == "cuelga":
        # Para comprobar que Rukes no se resiente: lo corta a los 2 s.
        time.sleep(60)
    if text == "muere":
        sys.exit(1)
    if not text:
        return [{"id": "ayuda", "title": "Escribe un texto para contarlo",
                 "subtitle": "cuenta <texto>", "actions": []}]
    words = len(text.split())
    ppm = int(config.get("palabras_por_minuto", 200)) or 200
    minutes = max(1, round(words / ppm))
    items = [
        {"id": "palabras", "title": f"{words} palabra" + ("" if words == 1 else "s"),
         "data": str(words)},
        {"id": "caracteres", "title": f"{len(text)} caracteres",
         "subtitle": f"{len(text.replace(' ', ''))} sin espacios", "data": str(len(text))},
        {"id": "lectura", "title": f"~{minutes} min de lectura",
         "subtitle": f"a {ppm} palabras por minuto", "data": str(minutes)},
    ]
    for it in items:
        it["actions"] = [{"id": "open", "label": "Copiar"}, {"id": "notify", "label": "Mostrar"}]
        it["detail"] = text
    return items


def action(params):
    item, act = params["item"], params["action"]
    if act == "notify":
        return {"notify": f"{item['id']}: {item['data']}"}
    return {"copy": item["data"]}


def main():
    global config
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        msg = json.loads(line)
        method, params, mid = msg.get("method"), msg.get("params") or {}, msg.get("id")
        try:
            if method == "initialize":
                config = params.get("config") or {}
                print(f"cuenta: iniciado (protocolo {params.get('protocol')})", file=sys.stderr)
                send({"id": mid, "result": {"name": "cuenta"}})
            elif method == "query":
                send({"id": mid, "result": {"items": query(params)}})
            elif method == "action":
                send({"id": mid, "result": action(params)})
            elif method == "cancel":
                pass  # Las consultas son instantáneas: nada que cancelar.
            elif mid is not None:
                send({"id": mid, "error": {"message": f"método desconocido: {method}"}})
        except Exception as e:  # noqa: BLE001 (un error no debe tumbar el plugin)
            if mid is not None:
                send({"id": mid, "error": {"message": str(e)}})


if __name__ == "__main__":
    main()
