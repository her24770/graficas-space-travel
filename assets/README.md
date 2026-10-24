# Cómo agregar un modelo

1. Exporta desde Blender: **File → Export → Wavefront (.obj)**
   - Marca **Triangulate Faces**.
   - Eje frontal **-Z Forward** y **Y Up** (es el valor por defecto de Blender).
   - El frente de la nave debe apuntar hacia **-Z**.
2. Guarda el archivo en `assets/models/` (por ejemplo `nave.obj`).
3. Listo: no hay que tocar código. Se detecta al compilar (`cargo run` o `make web-up`).

Notas:
- El modelo se **centra y se escala solo**, no importa el tamaño con el que lo hagas en Blender.
- Si existe `nave.obj`, es la nave que aparece al iniciar. Con **Tab** se cambia entre todos los modelos.
- El nombre del archivo se muestra en el título de la ventana.
- Si un archivo está mal formado, se muestra un cubo y el título dice `(ERROR: ...)`. El CI también falla.
- Mantén pocos triángulos (unos miles): el renderer corre en CPU.
