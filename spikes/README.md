# Spikes de la fase 0 (desechables)

Prototipos para elegir el motor de UI ([ADR 0001](../docs/decisions/0001-motor-ui.md)). Es un
workspace aparte: no forma parte del build de Rukes y no sigue sus lints.

| Crate | Qué es |
| --- | --- |
| `common` | Efectos DWM (Mica/Acrílico, esquinas, modo oscuro), medición de memoria y benchmark automático |
| `d2d` | Win32 + Direct2D/DirectWrite |
| `egui` | eframe 0.35 (`glow` por defecto, `--features wgpu`) |
| `iced` | iced 0.14 (`wgpu` por defecto, `--no-default-features --features tiny-skia`) |

## Ejecutar

```bash
cargo build --release -p spike-iced --no-default-features --features tiny-skia
SPIKE_NAME=iced-skia ./target/release/spike-iced.exe
```

| Variable | Efecto |
| --- | --- |
| `SPIKE_NAME` | Nombre del archivo `results/<nombre>.json` |
| `SPIKE_NOBENCH=1` | Solo muestra la ventana (Esc para cerrar) |
| `SPIKE_STAY=1` | No cierra al terminar el benchmark |
| `SPIKE_BACKDROP=acrylic` | Acrílico en lugar de Mica |

El benchmark tarda unos 10 s. Durante ese tiempo la ventana aparece y desaparece y toma el foco.
