# @rukes.name        {{name}}
# @rukes.description Describe aquí lo que hace
# @rukes.alias       {{alias}}
# @rukes.arg         texto:text
# @rukes.output      notification
# @rukes.timeout     30s
#
# Los argumentos llegan en el orden de @rukes.arg (y en RUKES_ARG_<NOMBRE>).
# Variables: RUKES_QUERY, RUKES_SELECTED_FILE, RUKES_CLIPBOARD, RUKES_ACTIVE_WINDOW.
# Salida: notification (última línea), silent, launcher, clipboard o terminal.

import os
import sys

texto = sys.argv[1] if len(sys.argv) > 1 else ""
print(f"Hola desde {os.environ.get('RUKES_SCRIPT_NAME')}: {texto}")
