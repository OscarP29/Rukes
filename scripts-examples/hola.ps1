# @rukes.name        Saludar
# @rukes.description Ejemplo: escribe «hola Ana» y pulsa Enter
# @rukes.alias       hola
# @rukes.arg         nombre:text:default=mundo
# @rukes.output      notification
# @rukes.timeout     10s
#
# Los argumentos llegan en el orden de @rukes.arg y también como $env:RUKES_ARG_<NOMBRE>.
# Edítalo con Ctrl+E desde el launcher o crea otro con «new script».

param([string]$nombre = "mundo")

Write-Output "¡Hola, $nombre!"
