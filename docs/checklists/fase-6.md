# Lista de verificación manual: Fase 6 (workflows y automatización)

Leyenda: ✅ verificado · ⏳ pendiente de verificar a mano · ⚠️ verificado con observaciones ·
➖ fuera de alcance

Desde esta fase las pruebas en la app las hace el usuario con la lista de abajo; las
automáticas (`cargo test`) cubren el formato, las posiciones, los triggers y el ejecutor.

## Criterios de aceptación

| Criterio | Estado | Notas |
| --- | --- | --- |
| Caso de uso 5: `modo trabajo` abre VS Code, el navegador con la documentación y la terminal, cada uno en su posición, en dos monitores | ⏳ | Prueba 2 |
| Los triggers programados se ejecutan | ⏳ | Prueba 6; tests: `next_due`, trigger `startup` |
| Los triggers se pueden pausar | ⏳ | Prueba 7 |

## Workflows (5.14)

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Pasos en TOML: app, url, script, wait, theme, power_plan, kill, notify | RF-150 | ⏳ | Tests de formato y del ejecutor (con ganchos simulados) |
| Posición por monitor, `snap`, coordenadas, tamaño y maximizado | RF-151 | ⏳ | Tests de `layout`; compensa el borde invisible de Windows 10/11 |
| Modos de ejemplo: Trabajo, Gaming, Presentación, Proyecto X | RF-152 | ✅ | `workflows/ejemplos.toml` (test) |
| Resultado en el launcher con alias y atajo propio | RF-154 | ⏳ | Pruebas 1 y 5 |
| Vista previa de los pasos e informe por paso | RF-155 | ⏳ | `Ctrl+P`; notificación al terminar y «Última ejecución» en la vista previa |
| Pasos condicionales | RF-153 | ➖ | Prioridad C |

## Triggers (5.15)

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Hora (con días), intervalo e inicio de sesión | RF-160 | ⏳ | `at`, `every` (mínimo 1 min), `on = "login"`/`"startup"` |
| Atajos globales por script (`@rukes.hotkey`) o workflow (`hotkey`) | RF-162 | ⏳ | Se liberan con «Pausar» de la bandeja |
| Pausa global desde la bandeja | RF-163 | ⏳ | «Pausar triggers» / «Reanudar triggers» |
| Triggers por evento (USB, red, app) | RF-161 | ➖ | Prioridad C |

## Pruebas para el usuario

1. `workflows`: aparecen Modo trabajo, Modo gaming, Modo presentación y Proyecto X. Con
   `Ctrl+P` se ven los pasos numerados.
2. `modo trabajo` (o `work`) + Enter: VS Code a la izquierda del monitor 1, docs.rs a la
   derecha del monitor 2 y Windows Terminal a la derecha del monitor 1; al final, una
   notificación «4 pasos completados». Con un solo monitor, docs.rs va al monitor 1 y el informe
   lo avisa.
3. `proyecto x`: el Explorador se abre centrado.
4. `modo presentación`: el launcher cambia al tema claro (vuelve al reiniciar o al tocar la
   configuración).
5. Atajo propio: añade `hotkey = "Ctrl+Alt+9"` a `[workflow.proyecto_x]`, guarda y pulsa el
   atajo con el launcher cerrado.
6. Trigger: añade `[[trigger]]` con `every = "1m"` y `run = "workflow:proyecto_x"`; `triggers`
   muestra «próxima en …» y al minuto se abre el Explorador.
7. Bandeja → «Pausar triggers»: `triggers` dice «en pausa» y no se abre nada; «Reanudar».
8. Un error a propósito (`snap = "diagonal"`) muestra un aviso con archivo y línea.
