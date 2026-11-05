# Lista de verificación manual: Fase 7 (plugins externos y fondos de pantalla)

Leyenda: ✅ verificado · ⏳ pendiente de verificar a mano · ⚠️ verificado con observaciones ·
➖ fuera de alcance

Las pruebas en la app las hizo el usuario (todas confirmadas el 2026-11-04); las automáticas (`cargo test`)
cubren el manifiesto, el protocolo, los tiempos máximos, la cancelación, la desactivación tras
3 fallos, el ejemplo en Python y la lista de fondos.

## Criterios de aceptación

| Criterio | Estado | Notas |
| --- | --- | --- |
| Caso de uso 7: `wall` muestra el gestor de fondos con previsualización | ✅ | Pruebas 1–4 |
| Un plugin externo que se cuelga o muere no degrada la latencia del resto | ✅ | Test `hanging_plugin_does_not_delay_the_rest`; pruebas 7 y 8 |

## Plugins externos (5.16)

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Protocolo JSON por stdin/stdout (`initialize`, `query`, `results`, `action`, `cancel`) | RF-170 | ✅ | `docs/plugin-protocol.md`; tests con un plugin falso en PowerShell |
| Manifiesto `plugin.toml` con nombre, versión, ejecutable, palabras clave, prefijo y permisos | RF-171 | ✅ | Tests del manifiesto |
| Proceso aparte con tiempos máximos (2 s / 10 s) y desactivación tras 3 fallos | RF-172 | ✅ | Tests `hang_times_out_and_recovers`, `three_deaths_disable_until_restart` |
| Instalación y retirada en caliente | RF-173 | ✅ | Pruebas 5 y 9 |
| Ejemplos en Python y Rust | — | ✅ | `plugins-examples/cuenta`, `plugins-examples/color` |

## Fondos de pantalla

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Lista visual con miniaturas y previsualización | RF-180 | ✅ | Pruebas 1 y 2 |
| Elección al azar | RF-180 | ✅ | Prueba 3 |
| Cambio automático programado | RF-180 | ✅ | Prueba 4 |

## Pruebas para el usuario

1. `wall`: «Fondo actual», «Fondo al azar» y las imágenes con miniatura; `Ctrl+P` la muestra grande.
2. Enter sobre una imagen: cambia el fondo en todos los monitores y queda «✓ actual».
3. `wall random`: pone un fondo distinto del actual.
4. `interval = "1m"`: «Cambio automático» y el fondo cambia solo; Enter lo cambia ya.
5. Copiar `plugins-examples\cuenta` a la carpeta de plugins: notificación con los permisos.
6. `cuenta hola que tal`: 3 palabras, 13 caracteres; Enter copia; «Mostrar» avisa.
7. `cuenta cuelga`: el resto sigue respondiendo; a los 2 s, «Cuenta no responde».
8. `cuenta muere` ×3: desactivado; Enter lo reinicia.
9. `plugins`: estado y permisos; borrar la carpeta avisa de que se quitó.
10. Ejemplo en Rust: `color #ff8800` da hex, rgb y hsl.
