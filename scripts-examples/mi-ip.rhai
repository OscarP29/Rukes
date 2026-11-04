// @rukes.name        Mi IP pública
// @rukes.description Consulta tu IP pública y la copia al portapapeles
// @rukes.alias       mi ip
// @rukes.output      clipboard
// @rukes.timeout     10s
//
// Ejemplo de Rhai, el lenguaje embebido: se ejecuta dentro de Rukes, sin abrir procesos.

let ip = http_get("https://api.ipify.org");
ip.trim();
ip
