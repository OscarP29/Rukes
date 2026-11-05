// @rukes.name        {{name}}
// @rukes.description Describe aquí lo que hace
// @rukes.alias       {{alias}}
// @rukes.arg         texto:text
// @rukes.output      notification
// @rukes.timeout     30s
//
// Rhai se ejecuta dentro de Rukes, sin abrir procesos. API:
//   open(ruta_o_url)            run(programa, [args]) -> #{code, stdout, stderr}
//   clipboard()                 set_clipboard(texto)
//   notify(texto)               notify(título, texto)
//   sleep(ms)                   env(nombre)
//   http_get(url)               http_post(url, cuerpo)      http(#{method, url, body, headers})
//   windows()                   focus_window(título)        move_window(título, x, y, ancho, alto)
//   parse_json(texto)           to_json(valor)
//   note_add(texto)             todo_add(texto)             remind("15m" | "14:30", texto)
//   plugin_call(plugin, comando, #{args}) -> valor           plugin_commands() -> lista
// Variables: query, args (mapa por nombre), selected_file, active_window.
// El valor final se muestra según @rukes.output; una lista de mapas #{title, subtitle, open,
// copy, query} se muestra como resultados.

let texto = args.texto;
`Hola desde Rhai: ${texto}`
