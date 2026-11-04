# 0004. Persistencia de estado y cachés en JSON versionado

- Estado: aceptada
- Fecha: 2026-11-04

## Contexto
Hay que guardar la frecency, el historial y las cachés de índices (pregunta abierta 13.3:
binario propio o SQLite). En la fase 1 los volúmenes son pequeños: ~1 000 apps y unos miles de
entradas de frecency.

## Decisión
- Formato: JSON con `serde_json`, con un campo `version`. Se escribe de forma atómica (archivo
  temporal + renombrado).
- Ubicación: `%LOCALAPPDATA%\Rukes\state\` (frecency) y `%LOCALAPPDATA%\Rukes\cache\` (índices).
- Si un archivo está corrupto o tiene otra versión, se descarta y se reconstruye (RNF-012).
- `serde_json` se necesitará igualmente para el protocolo de plugins (fase 7), así que no añade
  una dependencia nueva a largo plazo.
- No se usa SQLite (~1 MB de binario, más RAM).

## Consecuencias
Simple y depurable. Si el índice de archivos (fase 2, hasta 500 000 entradas) hace que la carga
supere RNF-005, ese índice concreto pasará a `rkyv` o `bincode` en su propio ADR.
