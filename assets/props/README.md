# Objetos de superficie

Modelos que se reparten solos sobre los planetas (árboles, rocas, cristales, edificios...).

## Cómo agregar uno
1. En Blender: modelo **parado**, con **Y arriba** y la **base en el suelo** (el origen da igual: se ajusta solo).
2. **File → Export → Wavefront (.obj)** con **Triangulate Faces**.
3. Guárdalo aquí con el nombre que pide `engine/src/scenario.rs`. Hoy se usan:

| Archivo | Planeta | Zona |
|---|---|---|
| `arbol.obj` | AQUA | tierra baja |
| `roca.obj` | AQUA (montañas) y FERRO | montaña / cualquier tierra |
| `edificio.obj` | FERRO | tierra baja |
| `cristal.obj` | GLACIA | cualquier lugar |

Mientras un archivo no exista, en su lugar aparece un **cono de prueba**.

## Notas
- Se escala solo a la altura indicada en `scenario.rs` (`size`), así que no importa el tamaño en Blender.
- Mantén pocos triángulos (**50 a 300**): se dibujan cientos de copias por planeta.
- Para cambiar cuántos hay, dónde van, su tamaño o agregar un tipo nuevo, edita `props` del planeta en `engine/src/scenario.rs`.
- La posición la decide una **semilla**: en el juego se cambia con `R` y todos los objetos se reacomodan.
