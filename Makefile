COMPOSE ?= docker-compose
COMPOSE_FILES := -f compose.yaml

.PHONY: up down logs ps smoke check hindsight-up

up:
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

hindsight-up:
	$(COMPOSE) $(COMPOSE_FILES) -f compose.hindsight.yaml up --build -d
