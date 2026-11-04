# 0002. Edición 2024 y lints de workspace

- Estado: aceptada
- Fecha: 2026-11-04

## Contexto
La especificación pide «edición 2021 o superior», sin `unwrap`/`expect` fuera de tests y `unsafe` aislado en `rukes-platform`.

## Decisión
- Edición 2024 (toolchain local 1.93; `rust-version = 1.85`).
- `[workspace.lints]`: `clippy::unwrap_used`, `clippy::expect_used` y `clippy::undocumented_unsafe_blocks` en `deny`; `missing_docs` en `warn`. `clippy.toml` permite `unwrap/expect` en tests.
- Todos los crates salvo `rukes-platform` llevan `#![forbid(unsafe_code)]`.
- Perfil release con `lto = "fat"`, `codegen-units = 1`, `strip = true`, manteniendo `panic = "unwind"` (necesario para `catch_unwind`, RNF-010).

## Consecuencias
Las reglas de la sección 2.5 las impone el compilador, no la revisión manual. Compilación release más lenta a cambio de binario más pequeño.
