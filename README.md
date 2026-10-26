# graficas-space-travel

Sistema solar explorable con una nave, hecho con un renderer de software en Rust (sin GPU ni librerías 3D).

## Estructura

| Carpeta | Qué es |
|---|---|
| `engine/` | Librería del renderer y la lógica. Sin ventana, sin archivos, sin relojes. Recibe `dt`. |
| `native/` | Ventana con `minifb`. Es lo que corre `cargo run`. |
| `web/` | El mismo `engine` compilado a WASM y mostrado en un `<canvas>` (extra). |
| `server/` | Servidor WebSocket (axum) que reparte las posiciones de las naves (extra). |
| `protocol/` | Mensajes binarios entre navegador y servidor, sin dependencias. |
| `assets/models/` | Modelos `.obj`. Ver [assets/README.md](assets/README.md). |
| `deploy/` | Compose de producción. Ver [deploy/README.md](deploy/README.md). |

## Ejecutar

**Nativo** (versión principal):
```bash
cargo run --release     # o: make run
```

**Web con multijugador en Docker** (simula el servidor, sin instalar nada de WASM):
```bash
make web-up             # http://localhost:3220 (abre dos pestañas para ver a dos jugadores)
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
| `Enter` | Empezar la carrera (o revancha) |
| `Tab` | Cambiar de modelo de nave |
| `F` | Pantalla completa (web) |
| `Esc` | Salir (solo nativo) |

**Pantalla completa.** La ventana se puede agrandar y el juego se adapta: dibuja a una altura fija (540 px) y el ancho sigue la proporción de la ventana. En el navegador usa `F`, doble clic o `F11`. En nativo, el botón verde de la ventana (macOS) o maximizarla; el sistema no permite un modo de pantalla completa propio con `minifb`.

## La carrera

Un circuito de 10 puntos de control (anillos) que serpentea entre las órbitas de los planetas. El siguiente punto se ve amarillo y una flecha en el borde de la pantalla indica hacia dónde ir.

1. `Enter` inicia la cuenta regresiva: todos van a su casilla de salida y esperan.
2. Se cruzan los puntos **en orden**; gana quien completa los 10 primero.
3. Al terminar todos aparecen los resultados; `Enter` da la revancha.

En pantalla: velocidad, triángulos dibujados y objetos visibles (frustum culling), posiciones, minimapa del sistema, nombres y distancias de los planetas. Las esferas cambian de detalle según la distancia (LOD).

Sin servidor la carrera funciona igual, tú solo.

## Multijugador (solo web)
Cada navegador simula su propia nave y manda su posición al servidor ~20 veces por segundo; el servidor la reparte a los demás (máximo 8), lleva el reloj de la carrera y el del mundo (así todos ven los planetas en el mismo lugar). Quien deja de mandar posiciones (por ejemplo, pestaña en segundo plano) deja de verse a los 5 s. Si no hay servidor, se juega solo. El `cargo run` nativo es siempre de un jugador.

## Modelos y escenario
Para agregar naves basta soltar el `.obj` en `assets/models/`. Los planetas y el circuito se editan en `engine/src/scenario.rs`. Guía completa en [assets/README.md](assets/README.md).

## Pendiente
Integrantes, contribuciones, lista de características a evaluar y declaración de uso de IA.
