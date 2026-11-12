# Objetos de superficie

Modelos que se reparten sobre los planetas. Su posición sale de una semilla y de ruido, así que con la misma semilla siempre quedan en el mismo lugar. En el juego, `R` cambia la semilla.

| Archivo | Planeta | Zona |
|---|---|---|
| `arbol.obj` | AQUA | tierra baja |
| `roca.obj` | AQUA y FERRO | montañas en AQUA, cualquier lugar en FERRO |
| `edificio.obj` | FERRO | tierra baja |
| `hielo1.obj`, `hielo2.obj` | GLACIA | cualquier lugar |
| `volcan.obj` | IGNIS | cualquier lugar |

En esta carpeta también están otros modelos que no van sobre la superficie: `luna.obj`, `anillos.obj` (anillos de TITAN y disco de los agujeros negros), `meteorito.obj` y `ovni.obj`.

## Agregar un objeto

1. En Blender, hacer el modelo parado, con Y hacia arriba y la base en el suelo.
2. Exportar como `.obj` con Triangulate Faces y guardarlo aquí. Si tiene colores, también su `.mtl`.
3. Agregarlo en los `props` del planeta en `engine/src/scenario.rs`, con cuántos van, su tamaño y en qué zona.

El modelo se escala a la altura que dice `size`, así que el tamaño en Blender no importa. Si el archivo no existe, se dibuja un cono en su lugar. Es mejor que tenga pocos triángulos (entre 50 y 300), porque se dibujan muchas copias por planeta.

Los materiales que tienen emisión (`Ke`) en el `.mtl` brillan solos, sin depender de la luz. Así se hace la lava de los volcanes.
