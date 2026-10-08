# One entrypoint, so the git hooks, the humans and the agents all run the same
# commands. Language toolchains (Gradle, Xcode) join here as their code lands
# — a target with no code behind it is a promise nobody is keeping.

PRETTIER ?= npx --yes --prefer-offline prettier@latest

# Gradle needs a JDK. A Mac with only Android Studio has one inside the app.
STUDIO_JBR := /Applications/Android Studio.app/Contents/jbr/Contents/Home
export JAVA_HOME ?= $(shell [ -d "$(STUDIO_JBR)" ] && echo "$(STUDIO_JBR)")
GRADLE = cd android && ./gradlew -q
# swift-format ships with Xcode, so the Mac app adds no tool of its own.
SWIFT_SRC = macos/Package.swift macos/Sources macos/Tests
SWIFT_FORMAT = xcrun swift-format

.PHONY: help init fmt lint test check image apk lint-server lint-android lint-macos test-server test-android test-macos

help:
	@echo 'init   install git hooks, verify agent symlinks, point agent memory at docs/memory'
	@echo 'fmt    rewrite files -- the only target that edits anything'
	@echo 'lint   prettier --check, gofmt, go vet, ktlint, Android lint, swift-format'
	@echo 'test   the server suite (race detector), the Android and the macOS unit tests'
	@echo 'check  lint test -- what pre-push and CI run'
	@echo 'image  build the server Docker image as dhun:dev'
	@echo 'apk    build the signed release APK and archive it in local/'

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
	$(GRADLE) ktlintFormat
	$(SWIFT_FORMAT) format -i -r $(SWIFT_SRC)

lint: lint-server lint-android lint-macos

# The docs are linted here too: the server's CI job is the one that runs on every push.
lint-server:
	$(PRETTIER) --check .
	@out=$$(gofmt -l server); [ -z "$$out" ] || { echo "gofmt needed:"; echo "$$out"; exit 1; }
	cd server && go vet ./...

lint-android:
	$(GRADLE) ktlintCheck :app:lintDebug

lint-macos:
	$(SWIFT_FORMAT) lint --strict -r $(SWIFT_SRC)

test: test-server test-android test-macos

test-server:
	cd server && go test -race -count=1 ./...

test-android:
	$(GRADLE) :app:testDebugUnitTest

test-macos:
	cd macos && swift test

check: lint test

image:
	docker build -f deploy/Dockerfile -t dhun:dev .

apk:
	scripts/archive-apk.sh
