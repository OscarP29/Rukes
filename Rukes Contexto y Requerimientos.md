# Rukes: Contexto y Requerimientos

*Launcher de productividad y automatización para Windows 10/11, escrito en Rust, ultraligero, rápido y visualmente cuidado. Documento pensado como contexto y especificación para Claude Code.*

---

## 1. Visión y contexto

**Qué es.** Un launcher invocado por atajo global (por defecto `Alt+Space`) que permite buscar y abrir apps, archivos, carpetas, ventanas y procesos, y ejecutar comandos, scripts y flujos de trabajo. Es un alternativo a PowerToys Run, Flow Launcher y Raycast, con un foco claro: **automatización como ciudadano de primera clase**, consumo mínimo de recursos y estética moderna.

**Por qué existe.** PowerToys Run y Flow Launcher están hechos en .NET y consumen mucha RAM en reposo. Rust permite un proceso residente de pocos MB, arranque instantáneo y respuesta inmediata.

**Diferenciadores.**

- Scripts en cualquier lenguaje que aparecen solos en el launcher, con cabecera de metadatos (estilo Raycast Script Commands).
- Creación de scripts desde el propio launcher.
- Scripting embebido (Rhai o Lua) sin depender de PowerShell.
- Workflows y modos (trabajo, gaming, proyecto) que encadenan acciones.
- Temas en TOML con recarga en caliente y efectos nativos de Windows 11 (Mica/Acrílico).
- Plugins externos por JSON sobre stdin/stdout, en cualquier lenguaje.

**Usuario objetivo.** Personas técnicas que automatizan tareas, usan teclado más que ratón y valoran un sistema rápido, ligero y personalizable.

**Plataforma.** Windows 10 (22H2) y Windows 11, x86\_64. Arm64 es deseable pero no prioritario.

---

## 2. Instrucciones para Claude Code

### 2.1 Cómo trabajar en este proyecto

1. Trabaja **por fases** (sección 9). No avances a la siguiente fase sin que la actual compile, pase tests y cumpla sus criterios de aceptación.
2. Antes de añadir una dependencia, justifica su impacto en tamaño de binario, RAM y tiempo de compilación. Prefiere pocas crates y bien mantenidas.
3. El rendimiento es un requisito, no un extra. Mide (RAM en reposo, latencia de aparición, latencia de búsqueda) al cierre de cada fase y reporta los números.
4. Escribe tests unitarios para la lógica pura (ranking, parsing, calculadora, config). La UI y las llamadas a la API de Windows se prueban manualmente con una lista de verificación.
5. Si una decisión de diseño es ambigua, elige la opción más simple, documéntala en `docs/decisions/` (ADR corto) y sigue.
6. Mantén un `CHANGELOG.md` y un `README.md` actualizados.

### 2.2 Stack propuesto

| Área | Elección | Notas |
| --- | --- | --- |
| Lenguaje | Rust estable, edición 2021 o superior | `cargo clippy` y `cargo fmt` sin warnings |
| API de Windows | crate `windows` (oficial de Microsoft) | Evita `winapi` |
| UI | **`iced` 0.14 con renderer `tiny-skia`** (decidido en fase 0) | Ver 2.4 y `docs/decisions/0001-motor-ui.md`. Plan B: Direct2D/DirectWrite |
| Fuzzy matching | `nucleo` | El motor de Helix, muy rápido |
| Hotkey global | `global-hotkey` o `RegisterHotKey` directo |  |
| Config | `serde` + `toml` + `notify` (hot reload) |  |
| Calculadora | `fend-core` | Unidades, divisas, expresiones naturales |
| Scripting embebido | `rhai` (puro Rust) o `mlua` | Preferencia inicial: `rhai` |
| Info de sistema | `sysinfo` |  |
| Logs | `tracing` + `tracing-appender` | Rotación de archivos |
| Errores | `thiserror` (libs) y `anyhow` (bin) |  |
| Serialización de caché | `bincode` o `rkyv` | Arranque rápido |
| Hashes | `sha2`, `md-5`, `crc32fast` |  |
| IDs | `uuid` |  |

### 2.3 Estructura del repositorio (workspace de Cargo)

```
rukes/
├─ Cargo.toml              # workspace
├─ crates/
│  ├─ rukes-core/           # motor: búsqueda, ranking, trait Plugin, acciones
│  ├─ rukes-plugins/        # plugins integrados (apps, files, calc, shell...)
│  ├─ rukes-script/         # descubrimiento y ejecución de scripts + Rhai
│  ├─ rukes-config/         # config, temas, hot reload
│  ├─ rukes-platform/       # envoltorios de la API de Windows (DWM, shell, hotkey, iconos)
│  ├─ rukes-ui/             # ventana, render, animaciones
│  └─ rukes-app/            # binario principal (daemon + orquestación)
├─ themes/                 # temas incluidos (.toml)
├─ scripts-examples/       # scripts de ejemplo
├─ docs/
│  ├─ decisions/           # ADRs
│  └─ plugin-protocol.md
└─ tests/
```

### 2.4 Decisión abierta: motor de UI

Hacer un **spike** que muestre una ventana sin bordes con 8 filas, fondo Mica, esquinas redondeadas y fade-in, con cada opción, y medir RAM, tiempo de aparición y esfuerzo. Criterio de elección: RAM de la ventana visible bajo 40 MB, aparición bajo 50 ms y animaciones fluidas a 60 fps. Si ninguna librería lo cumple, usar Direct2D/DirectWrite.

### 2.5 Convenciones de código

- Nada de `unwrap()` ni `expect()` fuera de tests e inicialización probada.
- Todo el código `unsafe` aislado en `rukes-platform`, con un comentario `// SAFETY:` por bloque.
- Las operaciones lentas (disco, red, procesos) nunca bloquean el hilo de UI.
- Textos de interfaz externalizados para permitir i18n (español e inglés desde el inicio).
- Commits pequeños con mensajes convencionales (`feat:`, `fix:`, `perf:`, `docs:`).

---

## 3. Alcance

**Dentro del alcance (v1.0):** todo lo descrito en la sección 5, organizado en fases.

**Fuera del alcance (v1.0):**

- Soporte para macOS o Linux.
- Sincronización en la nube de configuración.
- Tienda o marketplace de plugins (solo carpeta local).
- Modificar la barra de tareas nativa de Windows.
- Asistente de IA integrado (posible extensión futura vía plugin externo).

---

## 4. Casos de uso principales

1. Pulso `Alt+Space`, escribo `chr`, Enter: abre Chrome.
2. Escribo `= 5 km a millas`: obtengo el resultado y Enter lo copia.
3. Escribo `> shell:startup`: abre la carpeta de inicio.
4. Escribo `new script`: se crea una plantilla, se abre en mi editor, y al guardar aparece en el launcher.
5. Escribo `modo trabajo`: abre VS Code, el navegador con mis docs y la terminal, cada uno en su posición.
6. Escribo `kill chrome`: lista procesos con su RAM y cierro el que elija.
7. Escribo `wall`: veo mi gestor de fondos de pantalla con previsualización.
8. Escribo `clip`: busco en el historial del portapapeles y pego.
9. Escribo `gh issue Mi bug`: ejecuta mi script con `Mi bug` como argumento.

---

## 5. Requisitos funcionales (RF)

*Prioridad: **M** = imprescindible (Must), **S** = importante (Should), **C** = deseable (Could).*

### 5.1 Invocación y ventana

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-001 | El launcher se invoca con un atajo global configurable (por defecto `Alt+Space`) incluso con otras apps en primer plano | M |
| RF-002 | La ventana aparece centrada (o en posición configurable) en el monitor donde está el cursor | M |
| RF-003 | La ventana se oculta con `Esc`, al perder el foco o al ejecutar una acción (comportamientos configurables) | M |
| RF-004 | El campo de búsqueda recibe el foco automáticamente al aparecer; el texto de la búsqueda anterior es configurable (limpiar, conservar o seleccionar todo) | M |
| RF-005 | Soporte multi-monitor y DPI alto con escalado correcto | M |
| RF-006 | Instancia única: lanzar un segundo proceso invoca la ventana del primero | M |
| RF-007 | Icono en la bandeja del sistema con menú: Mostrar, Configuración, Recargar, Pausar, Salir | S |
| RF-008 | Opción de iniciar con Windows (entrada en el registro o carpeta de inicio) | S |

### 5.2 Búsqueda y resultados

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-010 | Búsqueda difusa (fuzzy) con resultados actualizados en cada pulsación | M |
| RF-011 | Ranking combinado: coincidencia fuzzy + frecuencia/recencia de uso (*frecency*) | M |
| RF-012 | Navegación con flechas, `Tab`, `Ctrl+J/K`; `Enter` ejecuta la acción principal | M |
| RF-013 | Atajos de acción secundaria sobre el resultado (`Ctrl+Enter` abrir como administrador, `Ctrl+Shift+Enter` abrir carpeta contenedora, `Ctrl+C` copiar ruta, etc.) | M |
| RF-014 | Cada resultado muestra icono, título, subtítulo y acciones disponibles | M |
| RF-015 | Prefijos de plugin configurables (`>`, `=`, `?`, `:`, `/`) para restringir la búsqueda a un plugin | M |
| RF-016 | Palabras clave (*triggers*) de plugin sin prefijo (ej. `clip`, `kill`, `wall`) | M |
| RF-017 | Panel de previsualización a la derecha (imagen, texto, markdown, info de archivo), activable y configurable | S |
| RF-018 | Historial de consultas navegable con `↑` en un campo vacío | C |
| RF-019 | Los resultados de plugins lentos aparecen de forma incremental sin bloquear los rápidos; las búsquedas obsoletas se cancelan | M |
| RF-020 | Límite configurable de resultados visibles y scroll fluido | M |

### 5.3 Plugin: Aplicaciones

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-030 | Detecta apps del Menú Inicio (accesos directos .lnk), apps UWP/MSIX y ejecutables en el PATH | M |
| RF-031 | Los alias personalizados se definen en la configuración (`c` → Chrome, `code` → VS Code) | M |
| RF-032 | Abrir como administrador, abrir ubicación, copiar ruta, desinstalar (si aplica) | M |
| RF-033 | Lanzar con argumentos o perfil predefinido (ej. Chrome con perfil X, terminal en carpeta Y) | S |
| RF-034 | Reindexado automático al instalar o desinstalar aplicaciones | S |

### 5.4 Plugin: Archivos y carpetas

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-040 | Índice propio de archivos de las carpetas configuradas, con caché en disco y actualización incremental (watcher) | M |
| RF-041 | Integración opcional con la API de *Everything* si está instalado | S |
| RF-042 | Reglas de exclusión (patrones glob, carpetas ocultas, `node_modules`, `.git`) | M |
| RF-043 | Acciones: abrir, abrir con..., abrir carpeta contenedora, copiar ruta, abrir terminal aquí | M |
| RF-044 | Filtros por prefijo (`f:` archivos, `d:` carpetas) y por extensión (`.pdf`) | S |

### 5.5 Plugin: Ventanas y procesos

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-050 | Lista las ventanas abiertas con búsqueda difusa; Enter la trae al frente (restaurándola si está minimizada) | M |
| RF-051 | Lista los procesos con uso de RAM y CPU; acción de terminar proceso con confirmación | M |
| RF-052 | `kill <nombre>` filtra procesos para terminarlos; terminar árbol de procesos como opción | S |
| RF-053 | Cerrar, minimizar y mover ventana a otro monitor desde el resultado | C |

### 5.6 Plugin: Shell y comandos del sistema

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-060 | `>` ejecuta comandos en el shell configurado (PowerShell, cmd, pwsh, nushell), con opción de mantener o cerrar la terminal | M |
| RF-061 | Soporta `shell:` y `ms-settings:` URIs (ej. `> shell:startup`) | M |
| RF-062 | Comandos del sistema: bloquear, suspender, hibernar, apagar, reiniciar, cerrar sesión, vaciar papelera, con confirmación configurable | M |
| RF-063 | Control de vorukesn, brillo, Wi-Fi, Bluetooth, modo claro/oscuro, modo avión (cuando la API lo permita) | S |
| RF-064 | Historial de comandos de shell con autocompletado | C |

### 5.7 Plugin: Calculadora y conversiones

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-070 | Expresiones matemáticas y lenguaje natural básico (`20% de 340`, `2^10`, `sqrt(144)`) | M |
| RF-071 | Conversión de unidades (`5 km a millas`, `70 F en C`) | M |
| RF-072 | Conversión de divisas con tasas en caché y actualización opcional en línea | S |
| RF-073 | Fecha y hora: zonas horarias (`hora en Tokio`), diferencias (`días hasta 25 dic`), formatos | M |
| RF-074 | `Enter` copia el resultado al portapapeles; opción de pegarlo | M |
| RF-075 | Conversor de colores: entrada `#ff5733`, `rgb()`, `hsl()` produce todas las representaciones con muestra visual | S |

### 5.8 Plugin: Generadores y utilidades de texto

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-080 | Generar UUID (v4/v7), contraseñas con longitud y reglas configurables, lorem ipsum, timestamps | M |
| RF-081 | Hashes (MD5, SHA-1, SHA-256, SHA-512, CRC32) de texto o de un archivo seleccionado | M |
| RF-082 | Codificar y decodificar Base64, URL, HTML, hex | S |
| RF-083 | Formateo rápido de JSON y diferencias entre dos textos | C |

### 5.9 Plugin: Web

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-090 | Abrir URLs escritas directamente | M |
| RF-091 | Búsquedas web con motores configurables y palabra clave (`g rust lifetimes`, `yt lofi`, `gh repo`) | M |
| RF-092 | Marcadores propios definidos en la configuración o importados de navegadores | C |

### 5.10 Plugin: Portapapeles con historial

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-100 | Registra texto copiado (y opcionalmente imágenes) en un historial local buscable | M |
| RF-101 | Elementos fijados que no expiran; límite y caducidad configurables | M |
| RF-102 | Lista de exclusión de aplicaciones (gestores de contraseñas) y detección del formato de «contenido sensible» de Windows para no registrarlo | M |
| RF-103 | Seleccionar un elemento lo pega en la ventana anterior (o lo deja en el portapapeles) | M |
| RF-104 | Historial almacenado cifrado o con permisos restringidos, y opción de desactivar el plugin por completo | S |

### 5.11 Plugin: Snippets y expansión de texto

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-110 | Snippets definidos en archivos TOML o Markdown con nombre, palabra clave y contenido | M |
| RF-111 | Variables dinámicas (`{fecha}`, `{hora}`, `{portapapeles}`, `{cursor}`) y campos rellenables | S |
| RF-112 | Insertar el snippet en la ventana anterior o copiarlo | M |
| RF-113 | Expansión automática global por abreviatura mientras se escribe (opt-in) | C |

### 5.12 Plugin: Notas y tareas

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-120 | `note <texto>` añade una nota con fecha a un archivo Markdown configurable | M |
| RF-121 | `todo <texto>` añade una tarea; `todo` lista las pendientes y permite completarlas con Enter | M |
| RF-122 | Los archivos son Markdown plano editable por el usuario, compatibles con Obsidian | S |
| RF-123 | Recordatorios simples con notificación (`remind 15m llamar a Ana`) | S |

### 5.13 Sistema de scripts (diferenciador clave)

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-130 | Carpeta `scripts/` (configurable) vigilada: todo script válido aparece en el launcher automáticamente, sin reiniciar | M |
| RF-131 | Lenguajes soportados: `.ps1`, `.bat`/`.cmd`, `.py`, `.js` (Node/Deno/Bun si existe), `.rhai`, `.lua` (si se activa), `.exe` y cualquier extensión mapeada a un intérprete configurable | M |
| RF-132 | Cabecera de metadatos en comentarios (nombre, descripción, icono, argumentos, modo de salida, atajo, alias, requiere admin, directorio de trabajo) | M |
| RF-133 | Argumentos con tipo (texto, número, opción, archivo, booleano) y valores por defecto; el launcher los solicita en línea (`gh issue Mi bug`) | M |
| RF-134 | Modos de salida: *silencioso*, *notificación*, *mostrar en el launcher*, *portapapeles*, *terminal visible* | M |
| RF-135 | Comando `new script` crea desde una plantilla (elegible por lenguaje), abre el editor configurado y lo registra al guardar | M |
| RF-136 | Edición y borrado de scripts desde el launcher (acciones secundarias) | S |
| RF-137 | Tiempo máximo de ejecución configurable por script y botón para cancelar | M |
| RF-138 | Captura de stdout y stderr en un historial de ejecuciones consultable (`logs`) | S |
| RF-139 | Scripting embebido con Rhai: API segura para abrir apps, mover ventanas, leer/escribir portapapeles, notificar, esperar, ejecutar comandos y HTTP | M |
| RF-140 | Scripts pueden devolver una lista de resultados en JSON para poblar el launcher de forma dinámica (*script filters*) | S |
| RF-141 | Variables de entorno inyectadas (`RUKES_QUERY`, `RUKES_SELECTED_FILE`, `RUKES_CLIPBOARD`, `RUKES_ACTIVE_WINDOW`) | S |

**Ejemplo de cabecera de script:**

```powershell
# @rukes.name        Crear issue en GitHub
# @rukes.description Abre un issue nuevo en el repo actual
# @rukes.icon        github
# @rukes.alias       gh issue
# @rukes.arg         title:text:required
# @rukes.output      notification
# @rukes.timeout     15s
```

### 5.14 Workflows y modos

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-150 | Un workflow es una secuencia declarativa de pasos en TOML (abrir app, abrir URL, ejecutar script, esperar, posicionar ventana, cambiar tema, cambiar plan de energía, cerrar procesos) | M |
| RF-151 | Posicionamiento de ventanas por workflow (monitor, coordenadas, tamaño, maximizado) | S |
| RF-152 | Modos predefinidos de ejemplo: *Trabajo*, *Gaming*, *Presentación*, *Proyecto X* | S |
| RF-153 | Pasos condicionales (si el proceso existe, si hay monitor externo, según la hora) | C |
| RF-154 | Cada workflow aparece como un resultado del launcher y se puede ejecutar con alias y atajo propio | M |
| RF-155 | Vista previa de los pasos antes de ejecutar y reporte de éxito o fallo por paso | S |

**Ejemplo de workflow:**

```toml
[workflow.trabajo]
name = "Modo trabajo"
alias = ["work", "trabajo"]
steps = [
  { app = "code", args = "C:/proyectos/actual", window = { monitor = 1, snap = "left" } },
  { url = "https://docs.rs", window = { monitor = 1, snap = "right" } },
  { script = "start-server" },
  { power_plan = "high_performance" },
]
```

### 5.15 Automatización y triggers (modo daemon)

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-160 | Triggers programados (hora, intervalo, al iniciar sesión) que ejecutan scripts o workflows | S |
| RF-161 | Triggers por evento: conexión de USB/monitor, cambio de red Wi-Fi, apertura de una app | C |
| RF-162 | Atajos globales adicionales por script o workflow | S |
| RF-163 | Los triggers se pueden pausar globalmente desde la bandeja | S |

### 5.16 Plugins externos

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-170 | Protocolo JSON sobre stdin/stdout con mensajes `query`, `results`, `action`, `initialize`, documentado en `docs/plugin-protocol.md` | M |
| RF-171 | Cada plugin externo declara un manifiesto (`plugin.toml`): nombre, versión, ejecutable, palabras clave, permisos | M |
| RF-172 | Los plugins externos corren en procesos separados con tiempo máximo de respuesta; un plugin que falla o se cuelga no afecta al launcher | M |
| RF-173 | Carga en caliente: instalar o quitar un plugin sin reiniciar | S |
| RF-174 | Los plugins integrados implementan el mismo trait que usa el motor internamente | M |

### 5.17 Integraciones propias

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-180 | Plugin `wall`: gestor de fondos de pantalla con lista visual, previsualización, aleatorio y programación (reutiliza el proyecto previo del autor) | S |
| RF-181 | Selector de tema global: cambia fondo, color de acento, modo claro/oscuro y tema de terminal con un comando | C |
| RF-182 | Modo `fetch`: muestra información del sistema con ASCII art | C |

### 5.18 Apariencia y temas

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-190 | Efecto Mica/Acrílico en Windows 11 mediante DWM; degradación elegante (color sólido con transparencia) en Windows 10 | M |
| RF-191 | Esquinas redondeadas y sombra nativas | M |
| RF-192 | Temas en TOML: colores, fuentes, radios, espaciados, opacidad, tamaño de icono, altura de fila | M |
| RF-193 | Recarga en caliente del tema al guardar el archivo | M |
| RF-194 | Temas incluidos: Catppuccin (4 variantes), Gruvbox, Nord, Tokyo Night, Dracula, además de uno claro y uno que sigue al sistema | M |
| RF-195 | Animaciones sutiles (aparición, selección, redimensionado) de 80-150 ms, desactivables; respetan la preferencia «reducir animaciones» de Windows | M |
| RF-196 | Cache de iconos en disco y en memoria con carga diferida | M |
| RF-197 | Soporte de fuentes con ligaduras e iconos (Nerd Fonts) y emojis a color | S |
| RF-198 | Sigue el modo claro/oscuro del sistema automáticamente (opcional) | S |

**Ejemplo de tema:**

```toml
[theme]
name = "Tokyo Night"
backdrop = "mica"          # mica | acrylic | solid
radius = 14
opacity = 0.92

[theme.colors]
background = "#1a1b26"
foreground = "#c0caf5"
accent = "#7aa2f7"
selection = "#283457"
muted = "#565f89"

[theme.font]
family = "JetBrains Mono"
size = 15
```

### 5.19 Configuración

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-200 | Configuración principal en `%APPDATA%\Rukes\config.toml`, con valores por defecto sensatos si falta el archivo | M |
| RF-201 | Recarga en caliente de la configuración, con mensaje claro (y sin caída) ante errores de sintaxis | M |
| RF-202 | Comando `config` abre el archivo en el editor; esquema JSON publicado para autocompletado en editores | S |
| RF-203 | Interfaz gráfica de ajustes mínima (opcional): atajo, tema, carpetas indexadas, plugins activos | C |
| RF-204 | Importar/exportar configuración (carpeta portable) y modo portable que guarda todo junto al ejecutable | S |
| RF-205 | Activar/desactivar y reordenar plugins; prioridad por plugin | M |

**Ejemplo de configuración principal:**

```toml
[general]
hotkey = "Alt+Space"
language = "es"              # es | en
theme = "tokyo-night"
start_with_windows = true
portable = false

[window]
position = "center"          # center | top-center | { x = 0.5, y = 0.3 }
width = 720
max_results = 8
hide_on_blur = true
on_show = "select_all"       # clear | keep | select_all
preview_panel = false
animations = true

[search]
frecency_weight = 0.35
history_size = 200

[plugins.apps]
enabled = true
priority = 100
aliases = { c = "Google Chrome", code = "Visual Studio Code" }

[plugins.files]
enabled = true
roots = ["~/Documents", "~/Desktop", "C:/proyectos"]
exclude = ["**/node_modules/**", "**/.git/**", "**/target/**"]
use_everything = "auto"      # auto | always | never

[plugins.shell]
prefix = ">"
shell = "pwsh"               # pwsh | powershell | cmd | nu
keep_open = false

[plugins.web.engines]
g  = "https://www.google.com/search?q={q}"
yt = "https://www.youtube.com/results?search_query={q}"
gh = "https://github.com/search?q={q}"

[scripts]
dir = "%APPDATA%/Rukes/scripts"
editor = "code"
default_timeout = "30s"
interpreters = { py = "python", js = "node", ts = "deno run" }
```

### 5.20 Internacionalización y accesibilidad

| ID | Requisito | Prio |
| --- | --- | --- |
| RF-210 | Interfaz disponible en español e inglés; idioma por defecto según el sistema, configurable | M |
| RF-211 | Textos en archivos de recursos (`.ftl` de Fluent o TOML) fuera del código; añadir un idioma no requiere recompilar lógica | S |
| RF-212 | Navegación 100 % con teclado; ninguna acción exige ratón | M |
| RF-213 | Exposición básica a lectores de pantalla (UI Automation: campo de búsqueda y elemento seleccionado) | C |
| RF-214 | Respeta escalado de texto y modo de alto contraste de Windows | S |

---

## 6. Requisitos no funcionales (RNF)

### 6.1 Rendimiento (se miden al cierre de cada fase)

| ID | Métrica | Objetivo | Cómo se mide |
| --- | --- | --- | --- |
| RNF-001 | RAM en reposo (ventana oculta, plugins por defecto) | ≤ 20 MB working set | Administrador de tareas / `sysinfo`, tras 10 min inactivo |
| RNF-002 | RAM con la ventana visible | ≤ 40 MB | Igual, con 8 resultados con icono |
| RNF-003 | Latencia hotkey → ventana visible y con foco | ≤ 50 ms (p95) | Timestamp en el hook vs. primer frame presentado |
| RNF-004 | Latencia pulsación → resultados actualizados (10 000 elementos) | ≤ 16 ms (p95) | Benchmark `criterion` + traza `tracing` |
| RNF-005 | Arranque en frío hasta «listo» (caché caliente) | ≤ 300 ms | Desde `main` hasta hotkey registrado e índice cargado |
| RNF-006 | CPU en reposo | ~0 % (sin *polling* activo; solo eventos) | Monitor de recursos durante 10 min |
| RNF-007 | Índice de archivos | 500 000 entradas en ≤ 60 MB de RAM adicional | Benchmark con árbol sintético |
| RNF-008 | Tamaño del binario (release, sin plugins externos) | ≤ 15 MB | `cargo build --release` con LTO y `strip` |
| RNF-009 | Animaciones | 60 fps sostenidos sin frames perdidos visibles | Contador de frames en modo debug |

### 6.2 Fiabilidad

- **RNF-010** Un fallo en un plugin (pánico, timeout, proceso caído) nunca tumba el launcher: los plugins integrados se ejecutan con `catch_unwind` y los externos en procesos aislados.
- **RNF-011** Errores de configuración o de tema se reportan con archivo, línea y mensaje; se mantiene la última configuración válida.
- **RNF-012** La caché corrupta o de versión incompatible se descarta y se reconstruye sin intervención del usuario.
- **RNF-013** El proceso aguanta 30 días de uptime sin crecimiento de memoria sostenido (sin fugas).
- **RNF-014** Logs rotativos en `%LOCALAPPDATA%\Rukes\logs` (máx. 5 archivos × 5 MB), nivel configurable.

### 6.3 Mantenibilidad y calidad

- **RNF-020** `cargo clippy -- -D warnings` y `cargo fmt --check` limpios en CI.
- **RNF-021** Cobertura de tests ≥ 70 % en `rukes-core`, `rukes-config` y `rukes-script` (lógica pura).
- **RNF-022** CI en GitHub Actions (`windows-latest`): build, tests, clippy, fmt y benchmarks de humo.
- **RNF-023** Documentación de la API pública de cada crate (`#![warn(missing_docs)]` en las libs).

### 6.4 Compatibilidad y distribución

- **RNF-030** Windows 10 22H2 y Windows 11 23H2+, x86\_64. Arm64 como objetivo de compilación opcional.
- **RNF-031** Sin dependencias en tiempo de ejecución (sin .NET, sin VC++ redistributable: enlazado estático del CRT).
- **RNF-032** Distribución como ZIP portable y como instalador (`winget` + MSI o Inno Setup) a partir de la v1.0.
- **RNF-033** Funciona sin privilegios de administrador; solo eleva bajo demanda para acciones concretas.

---

## 7. Arquitectura

### 7.1 Visión general

```
             ┌──────────────────────────── rukes-app (daemon) ────────────────────────────┐
 Hotkey ───▶ │  Event loop (hilo UI)  ◀──── canal ────▶  Motor de búsqueda (rukes-core)  │
 Bandeja     │   rukes-ui                                  │  ├─ Plugins integrados       │
 Triggers    │   └─ render, input, animaciones            │  ├─ Scripts (rukes-script)    │
             │                                            │  └─ Plugins externos (IPC)   │
             │  rukes-config (hot reload) ── notify ──▶  recarga de config/temas/scripts  │
             │  rukes-platform: DWM, hotkeys, shell, iconos, ventanas, portapapeles        │
             └───────────────────────────────────────────────────────────────────────────┘
```

- **Hilo de UI:** solo render y entrada. Nunca hace E/S.
- **Pool de trabajo:** `std::thread` + canales (`crossbeam-channel`) o un runtime `tokio` *current-thread* solo si los plugins externos y HTTP lo justifican (decisión en ADR durante la fase 1).
- **Cada consulta tiene un `QueryId` incremental;** los resultados de un `QueryId` obsoleto se descartan (RF-019).

### 7.2 Trait de plugin (borrador)

```rust
pub trait Plugin: Send + Sync {
    fn id(&self) -> &'static str;
    fn metadata(&self) -> PluginMetadata;          // nombre, prefijo, triggers, prioridad
    fn init(&mut self, ctx: &PluginContext) -> Result<()>;
    /// Devuelve resultados; debe respetar `cancel` y no bloquear más de lo necesario.
    fn query(&self, q: &Query, cancel: &CancelToken, sink: &ResultSink);
    fn execute(&self, item: &ResultItem, action: &ActionId, ctx: &ActionContext) -> Result<ActionOutcome>;
    fn preview(&self, _item: &ResultItem) -> Option<Preview> { None }
    fn on_config_change(&mut self, _cfg: &PluginConfig) {}
}
```

- `ResultItem`: `id`, `title`, `subtitle`, `icon`, `score`, `actions: Vec<Action>`, `plugin_id`, `payload` opaco.
- `ActionOutcome`: `Hide`, `KeepOpen`, `ReplaceQuery(String)`, `ShowResults(Vec<ResultItem>)`, `Notify(String)`.
- Los plugins externos se envuelven en un `ExternalPlugin` que implementa el mismo trait (RF-174).

### 7.3 Ranking

`score_final = fuzzy_score_normalizado × (1 − w) + frecency × w + bonus_alias + prioridad_plugin`

- *Frecency*: suma de usos ponderada por decaimiento exponencial (vida media configurable, por defecto 7 días), indexada por par `(consulta_normalizada, item_id)` y por `item_id`.
- Coincidencia exacta de alias o de prefijo de título recibe un *bonus* fijo.
- Desempates estables (por título) para que el orden no «baile» entre pulsaciones.

### 7.4 Persistencia

| Dato | Ubicación | Formato |
| --- | --- | --- |
| Configuración, temas, workflows, snippets | `%APPDATA%\Rukes\` | TOML / Markdown |
| Scripts del usuario | `%APPDATA%\Rukes\scripts\` | Archivos fuente |
| Plugins externos | `%APPDATA%\Rukes\plugins\<nombre>\` | `plugin.toml` + ejecutable |
| Caché de índices e iconos | `%LOCALAPPDATA%\Rukes\cache\` | `bincode`/`rkyv` versionado |
| Frecency, historiales | `%LOCALAPPDATA%\Rukes\state\` | Binario versionado o SQLite (ADR) |
| Portapapeles | `%LOCALAPPDATA%\Rukes\clipboard\` | Cifrado con DPAPI |
| Logs | `%LOCALAPPDATA%\Rukes\logs\` | Texto rotativo |

En modo portable todo vive en `.\data\` junto al ejecutable.

### 7.5 Protocolo de plugins externos (resumen)

JSON delimitado por líneas (*NDJSON*) sobre stdin/stdout; stderr va a los logs.

```jsonc
// launcher → plugin
{ "id": 1, "method": "initialize", "params": { "protocol": 1, "config": {}, "locale": "es" } }
{ "id": 2, "method": "query",      "params": { "query": "texto", "query_id": 42 } }
{ "id": 3, "method": "action",     "params": { "item_id": "abc", "action": "open" } }
{ "method": "cancel", "params": { "query_id": 42 } }

// plugin → launcher
{ "id": 2, "result": { "query_id": 42, "items": [ { "id": "abc", "title": "…", "subtitle": "…", "icon": "…", "actions": ["open"] } ] } }
{ "id": 3, "result": { "outcome": "hide" } }
{ "id": 2, "error": { "code": "timeout", "message": "…" } }
```

- Timeout por defecto: 2 s para `query`, 10 s para `action`. Tres fallos seguidos desactivan el plugin hasta recargarlo.
- Versión de protocolo negociada en `initialize`. Especificación completa en `docs/plugin-protocol.md`.

**Ejemplo de `plugin.toml`:**

```toml
[plugin]
name = "Jira"
version = "0.1.0"
executable = "jira-plugin.exe"
keywords = ["jira", "ticket"]
prefix = "j:"

[permissions]
network = true
filesystem = ["read"]
```

---

## 8. Seguridad y privacidad

- **Sin telemetría.** Ninguna conexión de red salvo las que el usuario activa (divisas, HTTP en scripts, plugins con permiso `network`, búsqueda de actualizaciones opcional).
- **Portapapeles:** cifrado en reposo con DPAPI (vinculado al usuario de Windows), respeto a `ExcludeClipboardContentFromMonitorProcessing` y `CanIncludeInClipboardHistory`, lista de exclusión de apps (RF-102, RF-104).
- **Scripts y plugins:** se ejecutan con los privilegios del usuario. La API Rhai es una *sandbox*: sin acceso a archivos ni procesos fuera de las funciones expuestas, con límites de operaciones y de tiempo. Los permisos de `plugin.toml` se muestran al instalar; en v1.0 son informativos (no hay aislamiento real a nivel de SO) y se documenta así.
- **Elevación:** solo bajo demanda (`runas`) por acción; el launcher nunca corre elevado por defecto.
- **Acciones destructivas** (terminar procesos, apagar, borrar scripts, vaciar papelera) piden confirmación por defecto.
- **Rutas e inyección:** argumentos de scripts y comandos se pasan como argumentos separados, nunca concatenados en una línea de shell, salvo en el plugin `>` donde el usuario escribe el comando explícitamente.

---

## 9. Fases de desarrollo

Cada fase termina con: build limpio, tests verdes, lista de verificación manual completada, métricas de la sección 6.1 reportadas y entrada en `CHANGELOG.md`.

### Fase 0 — Cimientos y spike de UI

- Workspace de Cargo con los crates vacíos, CI, `clippy`/`fmt`, `tracing`.
- Spike de UI (sección 2.4) con `iced`, `egui` y Direct2D; ADR con la decisión y las mediciones.
- **Aceptación:** ADR `0001-motor-ui.md` publicado; ventana de prueba con Mica, esquinas redondeadas y fade-in cumple los objetivos de 2.4.

### Fase 1 — MVP: invocar, buscar, abrir

- RF-001 a RF-006, RF-010 a RF-016, RF-019, RF-020, RF-030 a RF-032, RF-190, RF-191, RF-200, RF-201, RF-205, RF-212.
- Trait `Plugin` estable, motor con cancelación, frecency persistente, caché de iconos básica (RF-196).
- **Aceptación:** caso de uso 1 funciona; RNF-001, RNF-003, RNF-004 y RNF-005 cumplidos.

### Fase 2 — Plugins de utilidad

- Calculadora y conversiones (RF-070, RF-071, RF-073, RF-074), shell y sistema (RF-060 a RF-062), web (RF-090, RF-091), ventanas y procesos (RF-050 a RF-052), generadores (RF-080 a RF-082), archivos (RF-040, RF-042, RF-043).
- Bandeja e inicio con Windows (RF-007, RF-008).
- **Aceptación:** casos de uso 2, 3 y 6 funcionan; RNF-007 cumplido.

### Fase 3 — Sistema de scripts

- RF-130 a RF-141, con Rhai embebido y plantillas por lenguaje.
- **Aceptación:** casos de uso 4 y 9 funcionan; un script que se cuelga se cancela por timeout sin afectar al launcher.

### Fase 4 — Apariencia completa e i18n

- RF-192 a RF-198, RF-017 (previsualización), RF-210, RF-211, RF-214.
- **Aceptación:** los temas incluidos cargan y se recargan en caliente; inglés y español completos; Windows 10 degrada correctamente.

### Fase 5 — Productividad personal

- Portapapeles (RF-100 a RF-104), snippets (RF-110 a RF-112), notas y tareas (RF-120 a RF-123).
- **Aceptación:** caso de uso 8 funciona; contenido marcado como sensible no se registra.

### Fase 6 — Workflows y automatización

- RF-150 a RF-152, RF-154, RF-155, RF-160, RF-162, RF-163.
- **Aceptación:** caso de uso 5 funciona con posicionamiento en dos monitores; triggers programados se ejecutan y se pueden pausar.

### Fase 7 — Plugins externos e integraciones

- RF-170 a RF-173, `docs/plugin-protocol.md`, plugin de ejemplo en Python y en Rust; RF-180 (`wall`).
- **Aceptación:** caso de uso 7 funciona; un plugin externo que se cuelga o muere no degrada la latencia del resto.

### Fase 8 — Release 1.0

- Requisitos **S** pendientes, RF-202, RF-204, RNF-032 (instalador, `winget`), firma de código si es viable, README con capturas, documentación de usuario.
- Requisitos **C** solo si no comprometen la fecha; el resto pasa al backlog post-1.0.
- **Aceptación:** todos los requisitos **M** cumplidos, todos los RNF de 6.1 cumplidos en una máquina de referencia, 0 bugs críticos abiertos.

---

## 10. Estrategia de pruebas

- **Unitarias:** ranking y frecency, parsing de cabeceras de scripts, parsing de workflows y config, calculadora (casos en español e inglés), generadores, codificadores, protocolo de plugins (serialización).
- **Integración:** motor con plugins simulados (lentos, que fallan, que entran en pánico); plugin externo de prueba que no responde; recarga en caliente de config inválida.
- **Benchmarks (`criterion`):** búsqueda sobre 1k / 10k / 100k elementos; carga de caché; construcción de índice de archivos.
- **Manual (lista de verificación por fase en `docs/checklists/`):** multi-monitor con DPI distintos, juego a pantalla completa en primer plano, Windows 10 vs 11, modo oscuro/claro, reducir animaciones, apps UWP, rutas con espacios y Unicode.
- **Máquina de referencia** para las métricas: documentar CPU, RAM, disco y versión de Windows en `docs/benchmarks.md`.

---

## 11. Riesgos y mitigaciones

| Riesgo | Impacto | Mitigación |
| --- | --- | --- |
| Ninguna librería de UI cumple RAM/latencia | Alto | Spike en fase 0; Direct2D/DirectWrite como plan B |
| `Alt+Space` ya lo usa Windows (menú de ventana) o PowerToys | Medio | Detectar conflicto al registrar y avisar; permitir cambiar el atajo |
| Robo de foco: Windows impide `SetForegroundWindow` desde segundo plano | Alto | Técnicas conocidas (`AttachThreadInput`, input simulado) aisladas en `rukes-platform` y probadas |
| Enumeración de apps UWP/MSIX lenta o incompleta | Medio | `PackageManager` de WinRT con caché e indexado en segundo plano |
| Antivirus marca hooks de teclado / expansión global | Medio | Expansión global opt-in (RF-113), binario firmado, sin hooks de bajo nivel por defecto |
| Alcance demasiado grande para v1.0 | Alto | Fases estrictas; los requisitos **C** se recortan primero |

---

## 12. Glosario

- **Frecency:** combinación de frecuencia y recencia de uso para ordenar resultados.
- **Mica / Acrílico:** materiales translúcidos de Windows 11 aplicados vía DWM (`DWMWA_SYSTEMBACKDROP_TYPE`).
- **Script filter:** script que devuelve resultados en JSON en lugar de ejecutar una acción final.
- **Trigger:** palabra clave sin prefijo que activa un plugin, o evento que lanza un workflow (según contexto).
- **ADR:** *Architecture Decision Record*, documento corto con una decisión de diseño y su motivo.

---

## 13. Preguntas abiertas

1. ~~**Nombre definitivo**~~ Resuelto: el proyecto se llama **Rukes** (crates `rukes-*`, rutas `%APPDATA%\Rukes`, variables `RUKES_*`, cabeceras `@rukes.*`).
2. ~~**Motor de UI**~~ Resuelto en fase 0: iced + tiny-skia (ADR 0001).
3. **Estado persistente:** binario propio vs. SQLite (`rusqlite` añade ~1 MB); decidir en fase 1.
4. **Runtime async:** hilos + canales vs. `tokio`; decidir en fase 1.
5. **Proyecto previo de fondos de pantalla (`wall`):** ubicación del código y si se reutiliza como crate o como plugin externo.
6. **Licencia** del proyecto (MIT/Apache-2.0 dual es lo habitual en Rust).
