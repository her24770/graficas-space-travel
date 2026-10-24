# Contexto de build: la raíz del repo. Binario estático (musl): la imagen final pesa pocos MB.
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
# Mini-workspace solo con lo que necesita el servidor: así cambiar el engine no recompila axum.
RUN printf '[workspace]\nresolver = "2"\nmembers = ["protocol", "server"]\n' > Cargo.toml
COPY Cargo.lock ./
COPY protocol protocol
COPY server server
RUN cargo build -p server --release

FROM alpine:3
COPY --from=build /src/target/release/server /usr/local/bin/space-server
ENV BIND_ADDR=0.0.0.0:8000
EXPOSE 8000
CMD ["space-server"]
