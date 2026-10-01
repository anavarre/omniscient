PREFIX ?= $(HOME)/.local
APP_ID := io.github.anavarre.Taskestra

.PHONY: build install uninstall test smoke run

build:
	cargo build --release

test:
	cargo test --release

smoke: build
	scripts/smoke.sh

run: build
	./target/release/taskestra

install: build
	install -Dm755 target/release/taskestra $(PREFIX)/bin/taskestra
	install -Dm644 data/$(APP_ID).desktop $(PREFIX)/share/applications/$(APP_ID).desktop
	install -Dm644 data/$(APP_ID).svg $(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
	-gtk-update-icon-cache -q -t $(PREFIX)/share/icons/hicolor 2>/dev/null
	-update-desktop-database -q $(PREFIX)/share/applications 2>/dev/null

uninstall:
	rm -f $(PREFIX)/bin/taskestra
	rm -f $(PREFIX)/share/applications/$(APP_ID).desktop
	rm -f $(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg
