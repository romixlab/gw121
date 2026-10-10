# Changelog

All notable changes to gw121_cli are recorded here, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). [FEATURES.md](FEATURES.md) holds the current status of
every feature; IDs in parentheses refer to it.

## [Unreleased]

## [0.3.0] - 2026-10-10

### Added

- `gw121` library crate: scan, connect, read one packet or stream them, typed readings and flags, errors as an
  enum (LIB-1).
- `gw121` Python module (`crates/gw121_py`, `pip install ./crates/gw121_py`): `scan()`, `read()`, `log()`
  iterator, `Meter`, `parse()`, type stubs (PY-1).

### Changed

- The repo is a Cargo workspace: `crates/gw121` (library), `crates/gw121_cli` (the `gw121` binary, same
  commands and output), `crates/gw121_py`; install with `cargo install --path crates/gw121_cli` or
  `just install` (LIB-1).

## [0.2.0] - 2026-10-09

### Added

- `gw121 scan [--all] [--timeout]`: find nearby 121GW meters (or any BLE device) (CLI-4).
- `gw121 read [--address] [--timeout]`: connect and print one reading (CLI-5).
- `gw121 log [--address] [--timeout] [--for] [--every] [--csv]`: connect and print readings continuously until
  Ctrl+C or `--for`, optionally also as CSV (CLI-6).
- The 121GW's 19-byte BLE packet format, ported from libsigrok's `eev121gw` driver: main/sub displays,
  bargraph, global flags (DMM-1, DMM-2).
- A BLE indication reassembler that resyncs packets out of a stream not aligned to 19-byte boundaries (BLE-2).
- BLE transport over `bluer`: scan, connect, find the data characteristic by its `indicate` property (BLE-1).
- `--json` on every command, `--version` with git SHA and build time, colour, a spinner while scanning or
  connecting, dynamic shell completion (`source <(COMPLETE=bash gw121)`) (CLI-1..CLI-3).

### Fixed

- `read` and `log` give up after `--timeout` with "is the meter on, in BLE mode and in range?" instead of hanging
  when the meter is off but BlueZ still remembers it; `log --for` stops on time, and a meter that disconnects
  ends `log` with an error instead of a loop of warnings (BLE-1, CLI-6).
- `read` and `log` disconnect when done: a meter left connected stops advertising, so the next scan could not
  hear it; `scan` says `connected` or `not heard` where it has no RSSI, and connecting stops scanning at the
  first 121GW found instead of after the whole `--timeout` (BLE-1, CLI-4).
- JSON values are rounded to the meter's digits (3.2971, not 3.2971000000000004) (CLI-5, CLI-6).
- Agent skill `skills/gw121/SKILL.md` (SKILL-1).
