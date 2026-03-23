BINARY     := amux
INSTALL_PATH ?= /usr/local/bin

.PHONY: all build install test clean release

all: build

build:
	cargo build --release

install: build
	install -m 755 target/release/$(BINARY) $(INSTALL_PATH)/$(BINARY)

test:
	cargo test

clean:
	cargo clean

release:
	@if [ -z "$(VERSION)" ]; then \
		echo "Usage: make release VERSION=vx.y.z"; \
		exit 1; \
	fi
	@echo "==> Preparing release $(VERSION)..."
	@# Ensure we are on main and up to date
	git checkout main
	git pull --ff-only
	@# Check for clean working tree
	@if [ -n "$$(git status --porcelain)" ]; then \
		echo "Error: working tree is not clean. Commit or stash changes first."; \
		exit 1; \
	fi
	@# Create release notes file
	mkdir -p docs/releases
	@echo "# Release $(VERSION)" > docs/releases/$(VERSION).md
	@echo "" >> docs/releases/$(VERSION).md
	@echo "## Changes" >> docs/releases/$(VERSION).md
	@echo "" >> docs/releases/$(VERSION).md
	@echo "_Write release notes here._" >> docs/releases/$(VERSION).md
	@echo "==> Created docs/releases/$(VERSION).md"
	@echo "==> Launching amux chat to write release notes..."
	$(BINARY) chat
	@echo "==> Running tests..."
	cargo test
	@echo "==> Tests passed. Committing release notes..."
	git add docs/releases/$(VERSION).md
	git commit -m "Add release notes for $(VERSION)"
	@echo "==> Tagging $(VERSION)..."
	git tag "$(VERSION)"
	@echo "==> Pushing commit and tag to main..."
	git push origin main
	git push origin "$(VERSION)"
	@echo "==> Creating GitHub release..."
	gh release create "$(VERSION)" --title "$(VERSION)" --notes-file docs/releases/$(VERSION).md
	@echo "==> Release $(VERSION) complete!"
