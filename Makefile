COMPOSE := $(shell docker compose version >/dev/null 2>&1 && echo "docker compose" || echo "docker-compose")

.PHONY: run test web-up web-down web-logs

run:       ## Versión nativa (la que se entrega)
	cargo run --release

test:
	cargo test --workspace

web-up:    ## Versión web en Docker: http://localhost:3220
	$(COMPOSE) up --build -d

web-down:
	$(COMPOSE) down

web-logs:
	$(COMPOSE) logs -f web
