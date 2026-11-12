# graficas-space-travel

Sistema solar explorable con una nave, hecho con un renderizador de software propio en Rust (sin GPU ni librerías 3D). Se puede volar entre los planetas, acercarse a su superficie, correr una carrera multijugador o pelear contra ovnis.

## Video

[![Video del recorrido](https://img.youtube.com/vi/xeREr9CjO-s/0.jpg)](https://youtu.be/xeREr9CjO-s)

## Integrantes y contribuciones

| Integrante | Contribuciones |
|---|---|
| **Josué Hernández** | Estructura del proyecto (nativo y web), renderizador de software y carga de modelos OBJ, nave y cámara, escenario con sol y planetas, relieve de los planetas, multijugador con servidor WebSocket y despliegue, carrera, menú, sala de espera, minimapa, gravedad, colisiones con los planetas, objetos de superficie, modo órbita y primera persona. |
| **Jackelyn Girón** | Mapeo de normales, shaders (anillos, agujero negro, ovni, lava, luna y metal), materiales `.mtl`, modelos de naves y de superficie, luna, anillos, meteoritos, agujeros negros, ovnis y el planeta IGNIS, sistema de vida, disparos, modo ataque con puntos, colisiones con objetos, sonido, chispas y pantallas de derrota y victoria. |

## Cómo ejecutar

**Versión nativa.** Requiere [Rust](https://rustup.rs). En Linux, además, `libasound2-dev` para el sonido.
```bash
cargo run --release     # o: make run
```

**Versión web con multijugador** (opcional, con Docker):
```bash
make web-up             # abrir http://localhost:3220 (dos pestañas = dos jugadores)
make web-down
```

**Tests:** `make test` (o `cargo test --workspace`).

## Controles

| Tecla | Acción |
|---|---|
| `W` / `S` | Acelerar / reversa |
| `A` / `D` o `←` / `→` | Girar a la izquierda / derecha (en el menú: cambiar de nave) |
| `↑` / `↓` | Subir / bajar la nariz |
| `Espacio` | Frenar |
| `F` (mantener) | Disparar |
| `O` | Entrar o salir de la órbita del planeta más cercano |
| `G` | Gravedad sí / no |
| `V` | Primera / tercera persona |
| `R` | Nueva semilla: reacomoda los objetos sobre los planetas |
| `Tab` | Cambiar de nave |
| `Enter` | Jugar · empezar la carrera o la revancha · volver a jugar en el modo ataque |
| `P` | Modo ataque (desde el menú) |
| `M` | Volver al menú |
| `Esc` | Salir |

En la web, la pantalla completa es con doble clic o `F11`.

## Modos de juego

- **Exploración (sala de espera).** Al presionar `Enter` en el menú se entra a la sala: vuelo libre por todo el sistema solar para explorar los planetas. En la web se ve a los demás jugadores conectados.
- **Carrera.** Desde la sala, `Enter` inicia una carrera por 10 anillos que serpentean entre las órbitas. Gana quien los cruza todos primero, en orden. Chocar con un agujero negro u ovni, o quedarse sin vida, te elimina.
- **Modo ataque.** Desde el menú, `P`. Aparecen 5 ovnis que te persiguen y hay que destruirlos a disparos (6 cada uno). Se gana al destruir los 5; los puntos son 100 por ovni más un bonus por terminar rápido.

En todos los modos la nave tiene **vida**: los choques contra planetas, el sol u objetos la bajan según la fuerza del golpe.

## Planetas

| Planeta | Descripción |
|---|---|
| **Sol** | Estrella con superficie de fuego animada. Atrae a la nave. |
| **FERRO** | Rocoso y rojizo, con montañas, rocas y edificios. |
| **AQUA** | Parecido a la Tierra: mares, continentes, polos, bosques y rocas. Tiene una luna. |
| **TITAN** | Gigante gaseoso con bandas y anillos. |
| **GLACIA** | Planeta de hielo con grietas y cristales de hielo. |
| **NEBULA** | Gigante gaseoso morado y turquesa, con meteoritos que lo orbitan. |
| **IGNIS** | Planeta de lava con grietas de magma y volcanes que brillan. |

Además hay agujeros negros y ovnis que matan al jugador si los toca

## Características a evaluar y dónde verlas

| Característica | Dónde verla en el juego | Código |
|---|---|---|
| Planetas distinguibles que orbitan y rotan (6) | Todo el sistema; minimapa arriba a la derecha | `engine/src/scenario.rs` |
| Modelos propios sobre los planetas (árbol, roca, edificio, cristales de hielo, volcán) | AQUA, FERRO, GLACIA, IGNIS | `assets/props/` |
| Distribución procedural reproducible con semilla y ruido | Cualquier planeta con objetos; `R` cambia la semilla | `engine/src/props.rs` |
| Objetos bien orientados sobre la superficie curva mientras el planeta gira | Acercarse a AQUA o IGNIS | `engine/src/props.rs`, `engine/src/game/render.rs` |
| Shaders propios (sol, rocoso, gaseoso, hielo, anillos, lava, luna, agujero negro, ovni, metal de las naves) | Cada planeta y objeto | `engine/src/shaders.rs` |
| Shader animado | Sol, grietas de IGNIS, lava de los volcanes, luces de los ovnis | `engine/src/shaders.rs` |
| Mapeo de normales | Relieve fino de FERRO, AQUA y GLACIA | `engine/src/normalmap.rs` |
| Deformación de la geometría con ruido | Montañas de FERRO, AQUA y GLACIA (se ven en el contorno) | `engine/src/procedural.rs`, `engine/src/terrain.rs` |
| Generación y subdivisión procedural de geometría | Esferas de los planetas (icosaedro subdividido) y anillos de la carrera | `engine/src/procedural.rs` |
| Control local para rodear un planeta | Tecla `O` cerca de un planeta | `engine/src/orbit.rs` |
| Aceleración, frenado y giro progresivos según el tiempo | Al volar | `engine/src/ship.rs` |
| Atracción gravitatoria | Al pasar cerca de un planeta o del sol (`G` la apaga) | `engine/src/physics.rs` |
| Colisión que impide atravesar el planeta | Al chocar con un planeta (aviso y chispas) | `engine/src/game/collisions.rs` |
| Descarte de objetos fuera de la vista | Contador `OBJETOS visibles/total` arriba a la izquierda | `engine/src/frustum.rs` |
| Nivel de detalle según la distancia | Contador `TRIANGULOS`: baja al alejarse de un planeta | `engine/src/game/render.rs` |

Los modelos de las naves están en `assets/models/` y se pueden agregar más soltando un `.obj` en esa carpeta.