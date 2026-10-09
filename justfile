# gw121_cli: the EEVblog 121GW CLI and BLE client.

default:
    @just --list

# Tests (recorded packets; no BLE adapter or meter needed)
test:
    cargo test --quiet

# Format check and clippy without warnings
lint:
    cargo fmt --all --check
    cargo clippy --all-targets -- -D warnings

# Build and install gw121 on this PC; completion: source <(COMPLETE=bash gw121)
install:
    cargo install --quiet --path . --locked

# Bring this PC up to main
deploy:
    tpm repos pull gw121_cli
    just install
