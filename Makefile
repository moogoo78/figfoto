.PHONY: release dev test

# Builds the .deb and AppImage into ./release
release:
	docker compose run --rm bundle

# Runs the app with the GUI on the host display
dev:
	docker compose up dev

# Runs the Rust tests
test:
	docker compose run --rm sh cargo test --manifest-path src-tauri/Cargo.toml
