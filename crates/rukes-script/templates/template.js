// @rukes.name        {{name}}
// @rukes.description Describe aquí lo que hace
// @rukes.alias       {{alias}}
// @rukes.arg         texto:text
// @rukes.output      notification
// @rukes.timeout     30s
//
// Los argumentos llegan en el orden de @rukes.arg (y en RUKES_ARG_<NOMBRE>).
// Variables: RUKES_QUERY, RUKES_SELECTED_FILE, RUKES_CLIPBOARD, RUKES_ACTIVE_WINDOW.
// Salida: notification (última línea), silent, launcher, clipboard o terminal.

const texto = process.argv[2] ?? "";
console.log(`Hola desde ${process.env.RUKES_SCRIPT_NAME}: ${texto}`);
