# Deploy

El deploy es automático al hacer merge a `main`. Para ver otra rama en el servidor:
GitHub → **Actions → Deploy → Run workflow** → elige la rama.

Solo hay un ambiente: lo último que se despliegue es lo que se ve.
`/version.txt` en el sitio dice qué rama y commit hay en vivo.

## Configuración (una sola vez)

**En el servidor**
1. Crear la carpeta del proyecto (la que será `DEPLOY_PATH`) y dentro `dist/`.
2. Crear el archivo nginx del subdominio apuntando a `127.0.0.1:3220` (plantilla estándar).
3. Agregar la llave pública de deploy a `~/.ssh/authorized_keys`.
4. Primer arranque: ejecutar el deploy una vez; el workflow copia el compose y hace `docker compose up -d`.

**En GitHub → Settings → Secrets and variables → Actions**

| Nombre | Tipo | Valor |
|---|---|---|
| `SSH_PRIVATE_KEY` | secret | llave privada creada solo para deploy |
| `SSH_KNOWN_HOSTS` | secret | salida de `ssh-keyscan -H <host>` |
| `SSH_HOST` | secret | IP o dominio del servidor |
| `SSH_USER` | secret | usuario SSH |
| `DEPLOY_PATH` | secret | ruta absoluta de la carpeta en el servidor |
| `DEPLOY_URL` | variable (opcional) | `https://tu-subdominio` para la verificación final |

Crear la llave: `ssh-keygen -t ed25519 -f deploy_key -N "" -C "github-deploy"`.
