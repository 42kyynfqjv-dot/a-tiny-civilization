COMPOSE ?= docker compose
COMPOSE_FILES := -f compose.yaml

.PHONY: volumes up down logs ps smoke check proof-world hindsight-up production-preflight backend-status backup backup-status moderation-status restore-drill

volumes:
	./scripts/provision-runtime-volumes.sh

up: volumes
	$(COMPOSE) $(COMPOSE_FILES) up --build -d

down:
	$(COMPOSE) $(COMPOSE_FILES) down

logs:
	$(COMPOSE) $(COMPOSE_FILES) logs -f --tail=100

ps:
	$(COMPOSE) $(COMPOSE_FILES) ps

smoke:
	./scripts/smoke.sh

check:
	./scripts/check.sh

proof-world:
	@test -n "$(WORLD_ID)" || (echo "WORLD_ID is required" >&2; exit 2)
	@test -n "$(WORLD_SEED)" || (echo "WORLD_SEED is required" >&2; exit 2)
	$(COMPOSE) $(COMPOSE_FILES) exec -T runner /app/civilization-runner init-proof --world-id "$(WORLD_ID)" --seed "$(WORLD_SEED)"

hindsight-up: volumes
	$(COMPOSE) $(COMPOSE_FILES) -f compose.hindsight.yaml up --build -d

production-preflight:
	./scripts/production-preflight.sh

backend-status:
	./scripts/backend-status.sh

backup:
	./scripts/backup-postgres.sh

backup-status:
	./scripts/backup-status.sh

moderation-status:
	./scripts/moderation-status.sh

restore-drill:
	./scripts/restore-drill.sh
