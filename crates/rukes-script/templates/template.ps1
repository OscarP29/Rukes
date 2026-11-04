# @rukes.name        {{name}}
# @rukes.description Describe aquí lo que hace
# @rukes.alias       {{alias}}
# @rukes.arg         texto:text
# @rukes.output      notification
# @rukes.timeout     30s
#
# Los argumentos llegan en el orden de @rukes.arg (y en $env:RUKES_ARG_<NOMBRE>).
# Variables: RUKES_QUERY, RUKES_SELECTED_FILE, RUKES_CLIPBOARD, RUKES_ACTIVE_WINDOW.
# Salida: notification (última línea), silent, launcher, clipboard o terminal.
# Tipos de argumento: text, number, option(a|b), file, bool; añade :required o :default=x.

param([string]$texto = "")

Write-Output "Hola desde ${env:RUKES_SCRIPT_NAME}: $texto"
