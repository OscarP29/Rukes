@echo off
:: @rukes.name        {{name}}
:: @rukes.description Describe aquí lo que hace
:: @rukes.alias       {{alias}}
:: @rukes.arg         texto:text
:: @rukes.output      notification
:: @rukes.timeout     30s
::
:: Los argumentos llegan en el orden de @rukes.arg (%1, %2...) y en RUKES_ARG_<NOMBRE>.
:: Variables: RUKES_QUERY, RUKES_SELECTED_FILE, RUKES_CLIPBOARD, RUKES_ACTIVE_WINDOW.

echo Hola desde %RUKES_SCRIPT_NAME%: %~1
