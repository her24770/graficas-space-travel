# Modelos de naves

Las naves están en `assets/models/`: X-Wing (`xwing`), TIE Fighter (`tie`), `blueship` y `pinkship`. Cada una tiene su `.mtl` con los colores de sus partes.

Para agregar otra nave:

1. En Blender, exportar con File → Export → Wavefront (.obj), marcando Triangulate Faces. La nave tiene que mirar hacia -Z, con Y hacia arriba (es lo que Blender usa por defecto).
2. Guardar el `.obj` (y su `.mtl` si tiene colores) en `assets/models/`.
3. Compilar de nuevo. Los modelos se cargan solos al compilar, no hay que cambiar código.

Algunas cosas a tomar en cuenta:
- El modelo se centra y se escala al cargarlo, así que no importa el tamaño que tenga en Blender. Si queda volteado o muy pequeño, se ajusta en `AJUSTES_NAVE` en `engine/src/game.rs`.
- La nave inicial es la X-Wing. En el menú se cambia con `A`/`D` o `Tab`.
- Si dos jugadores usan la misma nave, a cada una se le pone un tinte con el color de su jugador.
- Si un `.obj` está mal formado, en su lugar se ve un cubo y los tests fallan.
- Conviene que tenga pocos triángulos (unos miles como máximo), porque todo se dibuja en CPU.

## Objetos sobre los planetas

Los árboles, rocas, edificios, hielos y volcanes están en `assets/props/`. Ver [props/README.md](props/README.md).

## Planetas y circuito

Los planetas, el sol y el circuito de la carrera se definen en [`engine/src/scenario.rs`](../engine/src/scenario.rs):

| Qué | Dónde |
|---|---|
| Planetas (tamaño, órbita, giro, material, gravedad, relieve, objetos, luna, anillos) | `PLANETS` |
| Sol | `SUN_RADIUS` y `SUN_GRAVITY` |
| Circuito | `course()`, `GATE_COUNT` y `GATE_RADIUS` |
| Posición de salida de cada jugador | `grid_slot()` |

Los shaders de cada material están en [`engine/src/shaders.rs`](../engine/src/shaders.rs).

`GATE_COUNT` también está en `protocol/src/lib.rs` y los dos deben tener el mismo valor (hay un test en `web` que lo revisa).
