.PHONY: check test build clean install

check:
	cargo fmt -- --check
	cargo check --all-targets --all-features
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all-targets --all-features
	RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features

test:
	cargo test --all-targets --all-features
	cargo test --test e2e_sshd -- --ignored
	cargo test --test perf_smoke -- --ignored

build:
	cargo build --release

clean:
	cargo clean

install:
	cargo install --path . --force
