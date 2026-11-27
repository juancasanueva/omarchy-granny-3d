PREFIX ?= $(HOME)/.local
BIN := granny3d
SHARE := $(PREFIX)/share/$(BIN)

.PHONY: build run test lint install uninstall

build:
	cargo build --release

run:
	cargo run --release

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings

# Installs for the current user so the game shows up in the Omarchy app launcher.
# The binary finds its models in $(PREFIX)/share/granny3d/assets.
install: build
	install -Dm755 target/release/$(BIN) $(PREFIX)/bin/$(BIN)
	install -d $(SHARE)/assets/models
	install -m644 assets/models/*.glb $(SHARE)/assets/models/
	install -Dm644 packaging/$(BIN).desktop $(PREFIX)/share/applications/$(BIN).desktop
	install -Dm644 packaging/$(BIN).svg $(PREFIX)/share/icons/hicolor/scalable/apps/$(BIN).svg
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null

uninstall:
	rm -f $(PREFIX)/bin/$(BIN)
	rm -rf $(SHARE)
	rm -f $(PREFIX)/share/applications/$(BIN).desktop
	rm -f $(PREFIX)/share/icons/hicolor/scalable/apps/$(BIN).svg
