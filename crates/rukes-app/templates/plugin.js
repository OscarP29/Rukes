// Plugin «{{name}}» de Rukes (protocolo: docs/plugin-protocol.md).
//
// Rukes lo arranca la primera vez que se usa y le habla por stdin/stdout con JSON, un mensaje
// por línea. Lo que escribas con console.error va al log de Rukes. Al guardar este archivo o
// plugin.toml, Rukes lo recarga solo.

const readline = require("readline");

let config = {};

function send(msg) {
  process.stdout.write(JSON.stringify(msg) + "\n");
}

// Resultados para lo escrito tras la palabra clave.
function query(params) {
  const text = params.text;
  if (!text) {
    return [{ title: "Escribe algo para probar «{{name}}»", actions: [] }];
  }
  return [
    {
      id: "mayusculas",
      title: text.toUpperCase(),
      subtitle: "Enter copia el texto en mayúsculas",
      data: text.toUpperCase(),
    },
  ];
}

// Lo que pasa al pulsar Enter: copy, open, notify, query, results, hide o keep_open.
function action(params) {
  return { copy: params.item.data };
}

const rl = readline.createInterface({ input: process.stdin });
rl.on("line", (line) => {
  const msg = JSON.parse(line);
  if (msg.id === undefined) return; // notificación «cancel»: no se responde
  try {
    let result;
    if (msg.method === "initialize") {
      config = msg.params.config || {};
      result = {};
    } else if (msg.method === "query") {
      result = query(msg.params);
    } else {
      result = action(msg.params);
    }
    send({ id: msg.id, result });
  } catch (e) {
    send({ id: msg.id, error: { message: String(e) } });
  }
});
rl.on("close", () => process.exit(0));
