# @rukes.name        Abrir proyecto
# @rukes.description Busca carpetas de proyectos y las abre (ejemplo de script filter)
# @rukes.alias       proj
# @rukes.filter      true
# @rukes.timeout     3s
#
# Un «script filter» se ejecuta mientras escribes: recibe el texto en RUKES_QUERY y devuelve
# resultados en JSON. Cada resultado puede tener "open" (ruta o URL), "copy" o "query".

import json
import os
from pathlib import Path

ROOT = Path.home() / "Desktop" / "Proyectos"
query = os.environ.get("RUKES_QUERY", "").lower()

items = []
if ROOT.is_dir():
    for d in sorted(ROOT.iterdir()):
        if d.is_dir() and query in d.name.lower():
            items.append({"title": d.name, "subtitle": str(d), "open": str(d)})

print(json.dumps({"items": items[:20]}))
