# Lista de verificación manual: Fase 8 (release 1.0)

Leyenda: ✅ verificado · ⏳ pendiente de verificar a mano · ⚠️ verificado con observaciones ·
➖ fuera de alcance

Las pruebas en la app las hizo el usuario (todas confirmadas el 2026-11-04, salvo Everything, que
no está instalado); las automáticas (`cargo test`) cubren perfiles, colores, análisis y formato de divisas, la
consulta y la respuesta de Everything, el esquema de la configuración, exportar e importar, y
los controles del sistema en modo solo lectura.

## Requisitos

| Caso | RF | Estado | Notas |
| --- | --- | --- | --- |
| Perfiles de apps con argumentos, carpeta y alias | RF-033 | ✅ | Test `profiles_match_by_name_and_alias_and_use_the_app_icon` |
| Reindexado de apps al cambiar el menú Inicio | — | ✅ | Vigilancia de `.lnk` con espera de 3 s |
| Integración con Everything | RF-041 | ⚠️ | Tests de la consulta y la respuesta; sin Everything instalado no se probó en vivo |
| Divisas con caché y actualización opcional en línea | RF-072 | ✅ | Descarga desactivada por defecto (sección 8, sin conexiones no activadas) |
| Conversor de colores con muestra | RF-075 | ✅ | Tests de conversión |
| Controles del sistema (volumen, brillo, radios, tema) | — | ✅ | Las escrituras no se ejecutan en los tests |
| Esquema `config.schema.json` | — | ✅ | Test `schema_describes_the_config` |
| Exportar e importar la configuración | RF-202 | ✅ | Tests de `transfer` |
| ZIP portable con `portable.txt` | RF-204 | ✅ | `scripts/package.ps1` |

## No funcionales y publicación

| Caso | Req. | Estado | Notas |
| --- | --- | --- | --- |
| CRT estático, sin redistribuibles | RNF-031 | ✅ | `.cargo/config.toml` |
| ZIP portable e instalador (Inno Setup) | RNF-032 | ⚠️ | ZIP generado en local; el instalador se genera en CI (Inno Setup no está instalado aquí) |
| Manifiestos de winget | RNF-032 | ⚠️ | Plantillas en `packaging/winget`; el PR a winget-pkgs se hace tras publicar |
| Instalación sin administrador | RNF-033 | ⚠️ | `PrivilegesRequired=lowest`; pendiente de probar el instalador |
| Icono y versión en el exe | — | ✅ | Propiedades → Detalles |
| Firma de código | — | ➖ | Sin certificado; `scripts/sign.ps1` y secretos de CI preparados |
| Binario ≤ 15 MB | RNF | ✅ | Comprobado en CI y en local |
| README con capturas y guía de usuario | — | ⚠️ | Faltan las imágenes de `docs/img/` |

## Pruebas manuales

Perfiles, reindexado, colores, divisas con y sin conexión, volumen, brillo, radios, tema de
Windows, esquema en VS Code, exportar e importar, icono y versión del exe y ZIP portable: ✅.
