# Contexto de build: la raíz del repo.
FROM rust:1-bookworm AS build
RUN rustup target add wasm32-unknown-unknown
WORKDIR /src

# wasm-bindgen-cli debe coincidir con la versión que fija Cargo.lock
COPY Cargo.lock ./
RUN VER=$(awk '/^name = "wasm-bindgen"$/{getline; gsub(/[" ]/,"",$3); print $3; exit}' Cargo.lock) \
    && cargo install wasm-bindgen-cli --version "$VER" --locked

COPY . .
RUN cargo build -p web --release --target wasm32-unknown-unknown \
    && wasm-bindgen --target web --out-dir /out/pkg target/wasm32-unknown-unknown/release/web.wasm \
    && cp web/www/* /out/

FROM nginx:alpine
COPY --from=build /out /usr/share/nginx/html
COPY web/nginx.conf /etc/nginx/conf.d/default.conf
