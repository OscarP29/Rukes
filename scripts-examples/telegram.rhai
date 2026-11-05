// @rukes.name        Bot de Telegram
// @rukes.description Lee los mensajes de tu bot y crea notas, tareas y recordatorios
// @rukes.alias       telegram
// @rukes.output      silent
// @rukes.timeout     20s
//
// Ejemplo de los comandos de plugins desde Rhai: note_add(), todo_add(), remind() y
// plugin_call(). Cada vez que se ejecuta lee los mensajes nuevos del bot, los procesa y los
// marca como leídos.
//
// Puesta en marcha:
//   1. Crea un bot con @BotFather y copia su token.
//   2. Crea la variable de entorno de usuario TELEGRAM_BOT_TOKEN con el token y reinicia Rukes.
//   3. Escribe cualquier cosa al bot y ejecuta «telegram»: te responde con tu chat id.
//   4. Crea TELEGRAM_CHAT_ID con ese número y reinicia Rukes. Solo se atienden mensajes de ese
//      chat: los de cualquier otra persona se ignoran.
//   5. Para que lo revise solo, añade en un archivo de workflows:
//        [[trigger]]
//        every = "1m"
//        run = "script:telegram"
//
// Mensajes que entiende:
//   /nota texto            (o cualquier texto sin /) añade una nota
//   /tarea texto           añade una tarea
//   /recordar 15m texto    recordatorio: 15m, 1h30m, 2d o 14:30
//   /tareas                tareas pendientes
//   /hecha texto           marca como hecha la tarea con ese texto
//   /recordatorios         recordatorios pendientes

let token = env("TELEGRAM_BOT_TOKEN");
let chat = env("TELEGRAM_CHAT_ID");
if token == "" {
    throw "Falta la variable de entorno TELEGRAM_BOT_TOKEN (ver el comentario del script)";
}
let api = `https://api.telegram.org/bot${token}`;

fn reply(api, chat, text) {
    http(#{
        method: "POST",
        url: `${api}/sendMessage`,
        headers: #{ "Content-Type": "application/json" },
        body: to_json(#{ chat_id: chat, text: text }),
    });
}

// `/recordar 15m té` → ["/recordar", "15m té"]
fn split_first(text) {
    let i = text.index_of(' ');
    if i < 0 {
        return [text, ""];
    }
    let rest = text.sub_string(i + 1);
    rest.trim();
    [text.sub_string(0, i), rest]
}

fn handle(text) {
    if !text.starts_with("/") {
        note_add(text);
        return "📝 Nota guardada";
    }
    let parts = split_first(text);
    let cmd = parts[0].to_lower();
    let rest = parts[1];
    // En los grupos llega como /nota@MiBot.
    let at = cmd.index_of('@');
    if at >= 0 {
        cmd = cmd.sub_string(0, at);
    }
    switch cmd {
        "/nota" | "/note" => {
            if rest == "" { return "Uso: /nota texto"; }
            note_add(rest);
            "📝 Nota guardada"
        }
        "/tarea" | "/todo" => {
            if rest == "" { return "Uso: /tarea texto"; }
            todo_add(rest);
            "✅ Tarea añadida"
        }
        "/recordar" | "/remind" => {
            let p = split_first(rest);
            if p[0] == "" { return "Uso: /recordar 15m texto (15m, 1h30m, 2d o 14:30)"; }
            let r = remind(p[0], p[1]);
            `⏰ Te lo recuerdo en ${p[0]}: ${r.text}`
        }
        "/tareas" | "/todos" => {
            let list = plugin_call("todo", "list");
            if list.is_empty() { return "No hay tareas pendientes"; }
            let out = `Pendientes (${list.len()}):`;
            for t in list { out += "\n• " + t.text; }
            out
        }
        "/hecha" | "/done" => {
            if rest == "" { return "Uso: /hecha texto de la tarea"; }
            if plugin_call("todo", "complete", #{ text: rest }) {
                "✔️ Hecha"
            } else {
                "No hay ninguna tarea pendiente con ese texto"
            }
        }
        "/recordatorios" | "/reminders" => {
            let list = plugin_call("remind", "list");
            if list.is_empty() { return "No hay recordatorios pendientes"; }
            let out = `Recordatorios (${list.len()}):`;
            for r in list { out += "\n• " + r.text; }
            out
        }
        _ => "Comandos: /nota, /tarea, /recordar 15m texto, /tareas, /hecha, /recordatorios",
    }
}

let res = parse_json(http_get(`${api}/getUpdates?timeout=0`));
if !res.ok {
    throw `Telegram: ${res.description}`;
}

let last = -1;
let done = 0;
for u in res.result {
    last = u.update_id;
    if !("message" in u) || !("text" in u.message) {
        continue;
    }
    let from = `${u.message.chat.id}`;
    if chat == "" {
        reply(api, from, `Tu chat id es ${from}. Guárdalo en la variable TELEGRAM_CHAT_ID.`);
        continue;
    }
    if from != chat {
        continue;
    }
    let text = u.message.text;
    text.trim();
    // Un mensaje que falla (p. ej. «/recordar pronto») responde con el error en vez de parar
    // el script: si no, se volvería a leer en cada ejecución.
    let answer = "";
    try {
        answer = handle(text);
    } catch (e) {
        answer = `⚠️ ${e}`;
    }
    reply(api, from, answer);
    done += 1;
}

// Marca como leídos los mensajes procesados.
if last >= 0 {
    http_get(`${api}/getUpdates?offset=${last + 1}&timeout=0`);
}
if done > 0 {
    print(`${done} mensaje(s) de Telegram procesados`);
}
