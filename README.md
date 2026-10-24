# graficas-space-travel

Sistema solar explorable con una nave, hecho con un renderer de software en Rust (sin GPU ni librerías 3D).

## Estructura

| Carpeta | Qué es |
|---|---|
| `engine/` | Librería del renderer y la lógica. Sin ventana, sin archivos, sin relojes. Recibe `dt`. |
| `native/` | Ventana con `minifb`. Es lo que corre `cargo run`. |
| `web/` | El mismo `engine` compilado a WASM y mostrado en un `<canvas>` (extra). |
| `assets/models/` | Modelos `.obj`. Ver [assets/README.md](assets/README.md). |
| `deploy/` | Compose de producción. Ver [deploy/README.md](deploy/README.md). |

## Ejecutar

**Nativo** (versión principal):
```bash
cargo run --release     # o: make run
```

**Web en Docker** (simula el servidor, sin instalar nada de WASM):
```bash
make web-up             # http://localhost:3220
make web-down
```
La primera vez tarda unos minutos (instala `wasm-bindgen-cli`); después queda en caché.

**Tests:** `make test`

## Controles

| Tecla | Acción |
|---|---|
| `W` / `S` | Acelerar / reversa |
| `A` / `D` | Girar izquierda / derecha |
| `↑` / `↓` | Subir / bajar la nariz |
| `Espacio` | Frenar |
| `Tab` | Cambiar de modelo de nave |
| `Esc` | Salir (solo nativo) |

## Agregar un modelo
Soltar el `.obj` en `assets/models/`. No hay que tocar código. Si se llama `nave.obj`, es la nave inicial.

## Pendiente
Integrantes, contribuciones, lista de características a evaluar y declaración de uso de IA.
