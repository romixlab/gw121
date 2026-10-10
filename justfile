# gw121: the EEVblog 121GW library, CLI and Python module.

py_venv := "/tmp/gw121_py-venv"

default:
    @just --list

# Tests (recorded packets; no BLE adapter or meter needed), Rust and Python
test: test-py
    cargo test --workspace --quiet

# Build the Python module (crates/gw121_py) into a venv under /tmp and run its pytest smoke test
[working-directory('crates/gw121_py')]
test-py:
    UV_PROJECT_ENVIRONMENT={{py_venv}} uv run --quiet --group dev pytest -q

# Format check and clippy without warnings, all crates
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# Build and install gw121 on this PC; completion: source <(COMPLETE=bash gw121)
install:
    cargo install --quiet --path crates/gw121_cli --locked

# Bring this PC up to main
deploy:
    tpm repos pull gw121
    just install
