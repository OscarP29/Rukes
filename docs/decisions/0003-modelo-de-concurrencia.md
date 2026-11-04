# 0003. Concurrencia: hilos y canales, sin tokio

- Estado: aceptada
- Fecha: 2026-11-04

## Contexto
Hay que decidir el runtime (pregunta abierta 13.4). Las búsquedas de los plugins integrados son CPU
y tardan microsegundos; la E/S (índices, iconos, config) es local. iced ya trae su propio ejecutor
(`thread-pool` de `futures`).

## Decisión
- El motor usa un pool fijo de hilos (`std::thread`) alimentado por `crossbeam-channel`.
- Cada consulta tiene un `QueryId` y un `CancelToken` (un `AtomicBool`). Una consulta nueva cancela
  la anterior. Los resultados obsoletos se descartan por `QueryId` en la UI (RF-019).
- Las tareas en segundo plano (escaneo de apps, carga de iconos, recarga de config) corren en hilos
  propios con nombre y se comunican con la UI por canal.
- No se añade tokio. Se reevalúa en la fase 7 (plugins externos por stdin/stdout y HTTP) si los
  hilos bloqueantes por plugin resultan insuficientes.

## Consecuencias
Menos dependencias y menos RAM en reposo. Los plugins deben comprobar el `CancelToken` en bucles
largos.
