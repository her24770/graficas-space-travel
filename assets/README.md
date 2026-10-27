# Cómo agregar un modelo de nave

1. Exporta desde Blender: **File → Export → Wavefront (.obj)**
   - Marca **Triangulate Faces**.
   - Eje frontal **-Z Forward** y **Y Up** (es el valor por defecto de Blender).
   - El frente de la nave debe apuntar hacia **-Z**.
2. Guarda el archivo en `assets/models/` (por ejemplo `nave.obj`).
3. Listo: no hay que tocar código. Se detecta al compilar (`cargo run` o `make web-up`).

Notas:
- El modelo se **centra y se escala solo**, no importa el tamaño con el que lo hagas en Blender.
- Si existe `nave.obj`, es la nave que aparece al iniciar. Con **Tab** se cambia entre todos los modelos.
- Cada jugador ve a los demás con **su** nave elegida (con otro color para distinguirlos).
- El nombre del archivo se muestra en el título de la ventana.
- Si un archivo está mal formado, se muestra un cubo y el título dice `(ERROR: ...)`. El CI también falla.
- Mantén pocos triángulos (unos miles): el renderer corre en CPU. El HUD muestra el contador de triángulos.

# Objetos sobre los planetas

Árboles, rocas, cristales, edificios... van en `assets/props/`. Instrucciones en [props/README.md](props/README.md).

# El escenario (planetas y circuito)

Todo está en [`engine/src/scenario.rs`](../engine/src/scenario.rs) y es solo datos:

| Qué | Dónde | Qué se puede cambiar |
|---|---|---|
| Planetas | `PLANETS` | nombre, radio, distancia al sol, período orbital, giro, inclinación, **material**, gravedad, relieve y objetos de superficie |
| Sol | `SUN_RADIUS` | tamaño (su aspecto es el material `Sun`) |
| Circuito | `course()` y `GATE_COUNT` | posición de cada punto de control y su tamaño (`GATE_RADIUS`) |
| Salida | `grid_slot()` | dónde se coloca cada jugador antes de largar |

Los materiales (shaders por píxel) están en [`engine/src/shaders.rs`](../engine/src/shaders.rs):
`Rocky` (mares y continentes), `Gas` (bandas), `Ice` (grietas), `Sun` (animado) y `Flat`/`Emissive`.

> Si cambias `GATE_COUNT`, cámbialo también en `protocol/src/lib.rs`. Un test en `web` avisa si no coinciden.
