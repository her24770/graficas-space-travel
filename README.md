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
| `A` / `D` o `←` / `→` | Girar izquierda / derecha |
| `↑` / `↓` | Subir / bajar la nariz |
| `Espacio` | Frenar |
| `Enter` | Jugar (en el menú) · empezar la carrera o la revancha (en la sala) · reiniciar (en modo ataque) |
| `P` | Modo ataque, un jugador (en el menú, con o sin conexión) |
| `M` | Volver al menú (desde la sala, los resultados o el modo ataque) |
| `G` | Gravedad sí / no |
| `O` | Entrar o salir de la órbita del planeta (o del sol) más cercano |
| `V` | Primera / tercera persona |
| `R` | Nueva semilla: reacomoda los objetos sobre los planetas |
| `Tab` | Cambiar de modelo de nave |
| `F` (mantener) | Disparar |
| `Esc` | Salir (solo nativo) |

**Pantalla completa.** La ventana se puede agrandar y el juego se adapta: dibuja a una altura fija (540 px) y el ancho sigue la proporción de la ventana. En el navegador usa doble clic o `F11`. En nativo, el botón verde de la ventana (macOS) o maximizarla; el sistema no permite un modo de pantalla completa propio con `minifb`.

## Menú y sala de espera

1. **Menú de inicio.** Al abrir el juego aparece el menú: la nave está quieta (no se mueve), la cámara recorre el sistema solar y se elige la nave con `A`/`D` o `Tab`. El panel de la derecha muestra si hay conexión y quién está en la sala. `Enter` = jugar.
2. **Sala de espera.** Después de jugar entras a la sala: todos están en el mismo mapa y pueden **volar libremente** (para practicar o explorar los planetas). Abajo a la izquierda se lista quién está en la sala. **Cualquiera** puede iniciar la carrera con `Enter`; `M` vuelve al menú.
3. **Carrera.** Al iniciar, **todos los que están en la sala de espera** pasan solos a la cuenta regresiva, se colocan en su casilla de salida y esperan. Después largan. Quien está en el menú o en el modo ataque no se entera: no corre y no pasa nada.
4. **Carrera en marcha.** Solo corre quien estaba en la sala al largar. Quien se conecta, o sale del menú o del modo ataque, cuando la carrera ya empezó **no entra tarde**: queda en la sala de espera con el aviso `CARRERA EN CURSO` hasta que termine. Mientras tanto solo ve a los que también esperan; los que corren solo ven a los que corren. Los aros no le cuentan y no puede iniciar otra carrera encima.

5. **Resultados.** Cuando todos terminan aparecen los tiempos. `Enter` = revancha, `M` = menú.

## Modo ataque (un jugador)

Desde el menú, `P` entra a un modo de un jugador, con o sin conexión: **5 ovnis cazadores** aparecen juntos delante de la nave y te persiguen. Hay que destruirlos a disparos (`F`); cada disparo suena con un láser. Arriba se ven el reloj, cuántos ovnis quedan, tus puntos y tu mejor partida.

- Los ovnis esperan 3 s al inicio para darte ventaja. Después aceleran poco a poco (de 35 a 80 u/s en 90 s; tu nave llega a 90) y giran lento, así que se les puede esquivar. Entre ellos no se enciman.
- Cada ovni aguanta 6 disparos; su vida se ve en una barrita encima.
- **Se gana al destruir los 5**: el reloj se detiene y aparece el cartel de victoria. `Enter` juega de nuevo.
- **Puntos:** 100 por ovni destruido más un bonus por ganar rápido (500 menos 5 por cada segundo, nunca negativo). La **mejor partida** es la de más puntos y, con los mismos puntos, la de menor tiempo; también cuenta si perdiste a medias.
- No atraviesan el sol, los planetas ni los agujeros negros: puedes esconderte detrás de un planeta, pero chocar contra él te quita vida.
- Una flecha roja con la distancia señala a cada ovni; avisa de `PELIGRO` si el más cercano se acerca y suena más fuerte cuanto más cerca.
- **Con conexión** también se juega, y es individual: cada jugador en modo ataque tiene su propia partida, no ve a nadie y nadie lo ve a él. Es independiente de la carrera: el servidor no lo espera para cerrarla y, si alguien larga una, sigue en su partida.
- Si un ovni te alcanza, chocas con un agujero negro o tu vida llega a 0, ves cuántos ovnis destruiste y la partida se reinicia sola. `Enter` reinicia ya, `M` vuelve al menú.

## La carrera

Un circuito de 10 puntos de control (anillos) que serpentea entre las órbitas de los planetas. El siguiente punto se ve amarillo y una flecha en el borde de la pantalla indica hacia dónde ir.

1. `Enter` inicia la cuenta regresiva: todos van a su casilla de salida y esperan.
2. Se cruzan los puntos **en orden**; gana quien completa los 10 primero.
3. Si chocas con un agujero negro o un ovni, o tu vida llega a 0, quedas **eliminado**: ves el cartel de "perdiste" con los aros y el tiempo que llevabas, y vuelves a la sala de espera, donde puedes volar libre mientras los demás terminan. Sales en la tabla como eliminado, pero los que siguen corriendo ya no ven tu nave.
4. Al terminar todos aparecen los resultados; `Enter` da la revancha.

En pantalla: velocidad, triángulos dibujados y objetos visibles (frustum culling), posiciones, minimapa del sistema, nombres y distancias de los planetas. Las esferas cambian de detalle según la distancia (LOD).

Sin servidor la carrera funciona igual, tú solo.

## Física y vistas

- **Gravedad.** El sol y cada planeta atraen la nave con `g·R²/d²` (`g` = gravedad en su superficie). El motor de la nave siempre es más fuerte que cualquier gravedad, así que siempre se puede escapar (se comprueba al compilar).
- **Vida.** Cada nave empieza con 100 %. Chocar contra el sol, un planeta, una luna, un meteorito o un objeto de superficie quita entre 3 y 30 puntos según la velocidad del golpe (un mismo choque no descuenta dos veces). En 0 la nave queda destruida y pierdes igual que con un agujero negro o un ovni, que siguen eliminando al instante. Tu vida se ve en una barra arriba a la izquierda y la de cada rival en una barrita sobre su nave; al reaparecer vuelve al 100 %.
- **Colisiones.** La nave no atraviesa el sol ni los planetas: choca contra la superficie **con su relieve**, rebota y se frena al deslizar. Un planeta en movimiento arrastra a la nave apoyada en él. En pantalla aparece `COLISION: <cuerpo>`.
- **Modo órbita (`O`).** Cerca de un cuerpo, la nave entra en órbita circular a la velocidad que da su gravedad (`v = √(g·R²/r)`). Los controles pasan a ser locales a ese cuerpo: `↑`/`↓` suben o bajan la órbita, `W`/`S` la aceleran o frenan y `A`/`D` giran su plano. `O` otra vez sale conservando la velocidad.
- **Primera persona (`V`).** La cámara va en la cabina y la nave propia no se dibuja, así nunca tapa la vista (no depende del modelo).
- **Relieve real.** Los planetas rocosos y de hielo tienen montañas de verdad: los vértices de la esfera se desplazan con el mismo ruido que pinta sus colores. Los gaseosos y el sol son lisos.
- **Mapa de normales.** Los planetas rocosos (FERRO, AQUA) y de hielo (GLACIA) tienen, además de sus montañas de geometría, relieve fino que la malla no tiene: lomas y cordilleras en la roca, fracturas en el hielo. Es mapeo de normales hecho en el rasterizador de software ([`normalmap.rs`](engine/src/normalmap.rs)): textura 1024×512 de normales en espacio tangente (horneada al arrancar desde un campo de alturas con ruido), coordenadas esféricas `u`/`v`, tangente por vértice interpolada con corrección de perspectiva, base TBN en el fragment shader, mipmaps y muestreo trilineal con el nivel calculado por triángulo (de lejos el relieve se aplana en vez de centellear). El mar queda liso. Siempre está activo; no agrega triángulos porque el detalle sale del sombreado, no de la geometría.
- **Luna.** AQUA tiene una luna (`assets/props/luna.obj`, definida con `moon` en [`scenario.rs`](engine/src/scenario.rs)) que orbita al planeta en un plano inclinado y gira sobre sí misma, con su propio shader de roca lunar (polvo, mares y cráteres). Es sólida: la nave choca contra ella.
- **Objetos de superficie.** Se reparten solos sobre los planetas con una **semilla** y **ruido** (bosques en tierra baja, rocas en montañas, nunca en el mar), parados perpendiculares al suelo y girando con el planeta. Ver [assets/props/README.md](assets/props/README.md).

## Parámetros para la demo

Todo está en [`engine/src/scenario.rs`](engine/src/scenario.rs):

| Qué | Dónde |
|---|---|
| Radio, distancia al sol, velocidad orbital (`orbit_period`) y giro de un planeta | `PLANETS` |
| Gravedad de cada planeta y del sol | `gravity`, `SUN_GRAVITY` |
| Altura de las montañas | `relief` (0 = liso) |
| Objetos de superficie: cuáles, cuántos, tamaño y zona | `props` de cada planeta |
| Semilla de los objetos | `PROPS_SEED` (o `R` en el juego) |
| Intensidad y colores de los shaders | `material` de cada planeta y [`shaders.rs`](engine/src/shaders.rs) |

## Para conectar efectos (choques)

Cada choque genera un `CollisionEvent` (cuerpo, punto, normal y velocidad de impacto) que se lee con `Game::take_collisions()`. Ahí se conectan los efectos visuales o de sonido; el juego solo resuelve el movimiento.

## Multijugador (solo web)
Cada navegador simula su propia nave y manda su posición al servidor ~20 veces por segundo; el servidor la reparte a los demás (máximo 8), lleva el reloj de la carrera y el del mundo (así todos ven los planetas en el mismo lugar). Quien deja de mandar posiciones (por ejemplo, pestaña en segundo plano) deja de verse a los 5 s. Si no hay servidor, se juega solo. El `cargo run` nativo es siempre de un jugador.

## Modelos y escenario
Para agregar naves basta soltar el `.obj` en `assets/models/`. Los planetas y el circuito se editan en `engine/src/scenario.rs`. Guía completa en [assets/README.md](assets/README.md).

## Pendiente
Integrantes, contribuciones, lista de características a evaluar y declaración de uso de IA.
