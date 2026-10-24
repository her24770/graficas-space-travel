# Deploy

Se despliega automáticamente al hacer merge a `main`. Para ver otra rama en el servidor:
GitHub → **Actions → Deploy → Run workflow** → elegir la rama.

Hay un solo ambiente: lo último que se despliegue es lo que se ve. `/version.txt` dice qué rama y commit están en vivo.

## Qué hace el workflow
1. Corre el CI (build + tests + WASM).
2. Compila la web (WASM) y construye la imagen Docker del servidor **en GitHub**, no en el servidor.
3. Sube `dist/` con `rsync` y la imagen con `docker save | ssh | docker load`.
4. Ejecuta `docker compose up -d` y verifica que el dominio responda.

## Configuración (una sola vez)

### 1. Llave SSH solo para deploy
```bash
ssh-keygen -t ed25519 -f deploy_key -N "" -C "github-deploy"
```
- La **pública** (`deploy_key.pub`) va en `~/.ssh/authorized_keys` del servidor.
- La **privada** (`deploy_key`) va como secreto en GitHub. No la subas al repo ni la reutilices.

### 2. Servidor
- Crear la carpeta del proyecto (será `DEPLOY_PATH`) y dentro `dist/`.
- Crear `/etc/nginx/sites-enabled/<nombre>` y recargar nginx:

```nginx
server {
    listen 80;
    server_name TU-SUBDOMINIO.jhgo.online;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    server_name TU-SUBDOMINIO.jhgo.online;
    ssl_certificate /etc/nginx/ssl/cloudflare.crt;
    ssl_certificate_key /etc/nginx/ssl/cloudflare.key;

    location /ws {
        proxy_pass http://127.0.0.1:3221;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 86400;
    }

    location = /health {
        proxy_pass http://127.0.0.1:3221;
    }

    location / {
        proxy_pass http://127.0.0.1:3220;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```
```bash
sudo nginx -t && sudo nginx -s reload
```

### 3. GitHub → Settings → Secrets and variables → Actions

| Nombre | Tipo | Valor |
|---|---|---|
| `SSH_PRIVATE_KEY` | secret | contenido de `deploy_key` |
| `SSH_KNOWN_HOSTS` | secret | salida de `ssh-keyscan -H <host>` |
| `SSH_HOST` | secret | IP o dominio del servidor |
| `SSH_USER` | secret | usuario SSH |
| `DEPLOY_PATH` | secret | ruta absoluta de la carpeta en el servidor |
| `DEPLOY_URL` | variable (opcional) | `https://TU-SUBDOMINIO.jhgo.online` |

Con `gh` se cargan así:
```bash
gh secret set SSH_PRIVATE_KEY < deploy_key
ssh-keyscan -H <host> | gh secret set SSH_KNOWN_HOSTS
gh secret set SSH_HOST --body "<host>"
gh secret set SSH_USER --body "<usuario>"
gh secret set DEPLOY_PATH --body "<ruta>"
gh variable set DEPLOY_URL --body "https://TU-SUBDOMINIO.jhgo.online"
```

## Seguridad
La llave de deploy da acceso como ese usuario del servidor (que además usa Docker). Por eso:
- Los secretos no se exponen a PRs de forks; el deploy solo corre desde `main` o a mano.
- Protege `main` (PR obligatorio y CI en verde) para que nadie despliegue algo sin revisar.
- Si la llave se filtra, bórrala de `authorized_keys` y genera otra.
