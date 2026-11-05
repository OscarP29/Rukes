// Documentación de Rukes: resaltado de código sin dependencias (funciona sin conexión), botón
// «Copiar» en cada bloque y marca de la sección visible en el índice.
(function () {
  "use strict";

  const STR_D = /"(?:\\.|[^"\\\n])*"/y;
  const STR_S = /'(?:\\.|[^'\\\n])*'/y;
  const STR_T = /"""[\s\S]*?"""/y;
  const STR_B = /`(?:\\.|[^`\\])*`/y;
  const NUM = /\b(?:0x[0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)(?:[a-z]{1,3}\b)?/y;
  const FUNC = /\b[A-Za-z_][\w]*(?=\s*\()/y;
  const IDENT = /\b[A-Za-z_][\w]*\b/y;
  const PUNCT = /[{}[\]();,.:=<>+\-*/%!&|^?#@]+/y;

  const words = (s) => new Set(s.split(/\s+/).filter(Boolean));

  // Cada lenguaje: reglas en orden; la primera que encaja en la posición actual gana.
  const LANGS = {
    toml: {
      label: "TOML",
      rules: [
        ["comment", /#.*/y],
        ["section", /^[ \t]*\[\[?[^\]\n]+\]\]?/my],
        ["key", /^[ \t]*[A-Za-z0-9_\-."]+(?=[ \t]*=)/my],
        ["string", STR_T], ["string", /'''[\s\S]*?'''/y], ["string", STR_D], ["string", STR_S],
        ["const", /\b(?:true|false)\b/y],
        ["number", NUM],
        ["key", /\b[A-Za-z_][\w-]*(?=\s*=)/y],
        ["punct", PUNCT],
      ],
    },
    rhai: {
      label: "Rhai",
      rules: [
        ["comment", /\/\/.*/y], ["comment", /\/\*[\s\S]*?\*\//y],
        ["string", STR_B], ["string", STR_D], ["string", STR_S],
        ["number", NUM],
        ["punct", /#\{/y],
      ],
      keywords: words("let const fn if else switch while loop for in do until break continue return throw try catch import export as private this is_def_fn"),
      consts: words("true false"),
    },
    rust: {
      label: "Rust",
      rules: [
        ["comment", /\/\/.*/y], ["comment", /\/\*[\s\S]*?\*\//y],
        ["string", /r#*"[\s\S]*?"#*/y], ["string", STR_D], ["string", /'(?:\\.|[^'\\])'/y],
        ["var", /'[a-z_]\w*\b/y],
        ["func", /\b[a-z_]\w*!/y],
        ["number", NUM],
      ],
      keywords: words("as async await break const continue crate dyn else enum extern fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait type unsafe use where while"),
      consts: words("true false None Some Ok Err"),
      types: /^[A-Z]/,
    },
    python: {
      label: "Python",
      rules: [
        ["comment", /#.*/y],
        ["string", /(?:[rbf]{0,2})"""[\s\S]*?"""/y], ["string", /(?:[rbf]{0,2})'''[\s\S]*?'''/y],
        ["string", /(?:[rbf]{0,2})"(?:\\.|[^"\\\n])*"/y], ["string", /(?:[rbf]{0,2})'(?:\\.|[^'\\\n])*'/y],
        ["number", NUM],
      ],
      keywords: words("and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield"),
      consts: words("True False None self"),
    },
    js: {
      label: "JavaScript",
      rules: [
        ["comment", /\/\/.*/y], ["comment", /\/\*[\s\S]*?\*\//y],
        ["string", STR_B], ["string", STR_D], ["string", STR_S],
        ["number", NUM],
      ],
      keywords: words("async await break case catch class const continue default delete do else export extends finally for from function if import in instanceof let new of return switch throw try typeof var void while yield require"),
      consts: words("true false null undefined this process console"),
    },
    powershell: {
      label: "PowerShell",
      rules: [
        ["comment", /<#[\s\S]*?#>/y], ["comment", /#.*/y],
        ["string", /@"[\s\S]*?"@/y], ["string", /@'[\s\S]*?'@/y],
        ["string", /"(?:`.|[^"`])*"/y], ["string", STR_S],
        ["var", /\$(?:\{[^}]*\}|[A-Za-z_][\w:]*)/y],
        ["keyword", /-[A-Za-z]+\b/y],
        ["func", /\b[A-Z][a-z]+-[A-Z][A-Za-z]+\b/y],
        ["number", NUM],
      ],
      keywords: words("param if else elseif foreach for while do switch function return try catch finally throw break continue in"),
      consts: words("true false null"),
      ci: true,
    },
    json: {
      label: "JSON",
      rules: [
        ["key", /"(?:\\.|[^"\\])*"(?=\s*:)/y],
        ["string", STR_D],
        ["const", /\b(?:true|false|null)\b/y],
        ["number", /-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b/y],
        ["punct", /[{}[\],:]/y],
      ],
    },
    ndjson: { alias: "json" },
    bash: {
      label: "Consola",
      rules: [
        ["comment", /#.*/y],
        ["string", STR_D], ["string", STR_S],
        ["var", /\$[A-Za-z_]\w*/y], ["var", /%[A-Za-z_]+%/y],
        ["keyword", /(?:^|(?<=\s))--?[A-Za-z][\w-]*/my],
      ],
    },
    text: { label: "Texto", rules: [] },
  };
  LANGS.ps1 = LANGS.powershell;
  LANGS.py = LANGS.python;
  LANGS.javascript = LANGS.js;
  LANGS.cmd = LANGS.bash;
  LANGS.ndjson = LANGS.json;

  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const span = (cls, s) => `<span class="t-${cls}">${esc(s)}</span>`;

  function highlight(src, lang) {
    const L = LANGS[lang];
    if (!L || !L.rules.length) return esc(src);
    let out = "";
    let i = 0;
    let plain = "";
    const flush = () => { if (plain) { out += esc(plain); plain = ""; } };
    outer: while (i < src.length) {
      for (const [cls, re] of L.rules) {
        re.lastIndex = i;
        const m = re.exec(src);
        if (m && m.index === i && m[0].length) {
          flush();
          out += span(cls, m[0]);
          i += m[0].length;
          continue outer;
        }
      }
      // Identificadores: palabra clave, constante, tipo o llamada a función.
      if (L.keywords || L.consts) {
        IDENT.lastIndex = i;
        const m = IDENT.exec(src);
        if (m && m.index === i) {
          const w = m[0];
          const k = L.ci ? w.toLowerCase() : w;
          FUNC.lastIndex = i;
          const f = FUNC.exec(src);
          let cls = null;
          if (L.keywords && L.keywords.has(k)) cls = "keyword";
          else if (L.consts && L.consts.has(k)) cls = "const";
          else if (f && f.index === i) cls = "func";
          else if (L.types && L.types.test(w)) cls = "type";
          flush();
          out += cls ? span(cls, w) : esc(w);
          i += w.length;
          continue;
        }
      }
      PUNCT.lastIndex = i;
      const p = PUNCT.exec(src);
      if (p && p.index === i && /^(?:->|=>|←|→)$/.test(p[0])) {
        flush();
        out += span("arrow", p[0]);
        i += p[0].length;
        continue;
      }
      plain += src[i];
      i += 1;
    }
    flush();
    return out;
  }

  // Líneas de protocolo: «→ {json}» y «← {json}».
  function highlightProtocol(src) {
    return src.split("\n").map((line) => {
      const m = /^(\s*)(→|←)(\s*)(.*)$/.exec(line);
      if (!m) return highlight(line, "json");
      return esc(m[1]) + span("arrow", m[2]) + esc(m[3]) + highlight(m[4], "json");
    }).join("\n");
  }

  function decorate(pre) {
    const code = pre.querySelector("code") || pre;
    const cls = [...code.classList].find((c) => c.startsWith("language-"));
    const lang = cls ? cls.slice(9) : "text";
    const src = code.textContent.replace(/\n$/, "");
    code.innerHTML = lang === "protocol" ? highlightProtocol(src) : highlight(src, lang);
    const box = document.createElement("div");
    box.className = "code";
    const bar = document.createElement("div");
    bar.className = "bar";
    const label = document.createElement("span");
    label.textContent = pre.dataset.title
      || (lang === "protocol" ? "Protocolo (NDJSON)" : (LANGS[lang] && LANGS[lang].label) || lang);
    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = "Copiar";
    btn.addEventListener("click", () => {
      navigator.clipboard.writeText(src).then(() => {
        btn.textContent = "Copiado";
        setTimeout(() => { btn.textContent = "Copiar"; }, 1400);
      });
    });
    bar.append(label, btn);
    pre.replaceWith(box);
    box.append(bar, pre);
  }

  function activeNav() {
    const links = [...document.querySelectorAll("nav a.item[href^='#']")];
    const targets = links
      .map((a) => document.getElementById(a.getAttribute("href").slice(1)))
      .filter(Boolean);
    if (!targets.length || !("IntersectionObserver" in window)) return;
    const byId = new Map(links.map((a) => [a.getAttribute("href").slice(1), a]));
    const io = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          links.forEach((a) => a.classList.remove("active"));
          const a = byId.get(e.target.id);
          if (a) a.classList.add("active");
        }
      }
    }, { rootMargin: "0px 0px -75% 0px" });
    targets.forEach((t) => io.observe(t));
  }

  document.addEventListener("DOMContentLoaded", () => {
    document.querySelectorAll("main pre").forEach(decorate);
    activeNav();
  });
})();
