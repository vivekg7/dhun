# One entrypoint, so the git hooks, the humans and the agents all run the same
# commands. Language toolchains (Gradle, Xcode) join here as their code lands
# — a target with no code behind it is a promise nobody is keeping.

PRETTIER ?= npx --yes --prefer-offline prettier@latest

.PHONY: help init fmt lint test check image

help:
	@echo 'init   install git hooks, verify agent symlinks, point agent memory at docs/memory'
	@echo 'fmt    rewrite files -- the only target that edits anything'
	@echo 'lint   prettier --check, gofmt, go vet'
	@echo 'test   the server suite, with the race detector'
	@echo 'check  lint test -- what pre-push and CI run'
	@echo 'image  build the server Docker image as dhun:dev'

# Writes the one setting that cannot be committed: autoMemoryDirectory takes an
# absolute path, so it lives in the git-ignored settings.local.json while the
# directory it points at is tracked and reviewed like any other doc. An
# existing file is never overwritten -- the maintainer's other local settings
# are theirs.
init:
	git config core.hooksPath .githooks
	@[ -L CLAUDE.md ] || ln -sf AGENTS.md CLAUDE.md
	@[ -L .claude/skills ] || ln -sf ../.agents/skills .claude/skills
	@echo 'agent symlinks verified (CLAUDE.md, .claude/skills)'
	@if [ -f .claude/settings.local.json ]; then \
		echo 'note: .claude/settings.local.json exists, leaving it alone'; \
		echo '      it must set "autoMemoryDirectory" to $(CURDIR)/docs/memory'; \
	else \
		printf '{\n  "autoMemoryDirectory": "%s/docs/memory"\n}\n' '$(CURDIR)' \
			> .claude/settings.local.json; \
		echo 'wrote .claude/settings.local.json -> docs/memory'; \
	fi
	@echo 'hooks installed -> .githooks'

fmt:
	$(PRETTIER) --write .
	gofmt -w server

lint:
	$(PRETTIER) --check .
	@out=$$(gofmt -l server); [ -z "$$out" ] || { echo "gofmt needed:"; echo "$$out"; exit 1; }
	cd server && go vet ./...

test:
	cd server && go test -race -count=1 ./...

check: lint test

image:
	docker build -f deploy/Dockerfile -t dhun:dev .
