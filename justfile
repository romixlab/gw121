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

# With tpm on PATH: tpm install (kept-releases stack, one copy on PATH); otherwise: cargo install
# Build and install gw121 on this PC; completion: source <(COMPLETE=bash gw121)
install:
    cargo build --profile deploy --locked -p gw121_cli
    if command -v tpm >/dev/null; then tpm install target/deploy/gw121; else cargo install --locked --path crates/gw121_cli; fi

# Bring this PC up to main: pull, install, check --version against main
deploy:
    if command -v tpm >/dev/null; then tpm repos pull gw121; else git pull --ff-only; fi
    just install
    gw121 --version | grep -q "($(git rev-parse --short HEAD)" || { echo "gw121 --version does not show $(git rev-parse --short HEAD)"; gw121 --version; exit 1; }
    gw121 --version
