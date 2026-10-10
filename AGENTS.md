# Working on gw121_cli

Guidance for AI agents and contributors. Read this before changing code.

gw121_cli is the `gw121` Rust library, the `gw121` command line tool and agent skill on top of it, and the
`gw121` Python module, for the EEVblog 121GW multimeter: reads the main and secondary displays, the bargraph,
and the meter's flags (auto-range, hold, relative, AC/DC, battery) over Bluetooth LE. Mode and range are chosen on the meter's own rotary knob; this tool only ever reads them, never
sets them — there is no "write" side to this protocol worth having.

The 121GW's BLE packet format is not published by EEVblog/UEi (the vendor's own "Revised-Packet-Format-Blob-V2"
PDF, linked from the product page, was not reachable while building this). The provenance chain this is built
on:

- The [sigrok wiki's EEVBlog 121GW page](https://sigrok.org/wiki/EEVBlog_121GW) explains the transport: the
  meter's BLE112/BLE122 module runs in cable-replacement mode, forwarding the STM32 main MCU's UART bytes
  verbatim as BLE indications (subscribe by writing `0x0003` to the CCCD of the characteristic at ATT handle 9;
  data arrives on handle 8), and says packets are 19 bytes of binary data covering three "displays": `main`,
  `sub`, `bar`.
- [libsigrok](https://github.com/sigrokproject/libsigrok)'s `src/dmm/eev121gw.c` (GPL-2.0-or-later, Gerhard
  Sittig, 2018) is the only published, byte-accurate decoder: it reverse-engineers the exact bit layout, mode
  and range numbers, and per-mode/range scale tables, cross-referenced against the vendor's printed manual's
  "Calibration Reference Table". `src/hardware/serial-dmm/api.c` confirms the serial framing (115200/8n1, no
  extra length/escape byte beyond the 19-byte packet itself), and `src/serial_bt.c` confirms the ATT handles
  (`ble122` profile: read handle 8, CCCD handle 9, CCCD value `0x0003` to enable both notify and indicate).
- The [`121gw-bluetooth-protocol`](https://eevblog.com/forum/testgear/121gw-bluetooth-protocol/) and
  [`121gw-bluetooth-logging-with-python`](https://www.eevblog.com/forum/testgear/121gw-bluetooth-logging-with-python)
  EEVblog forum threads are the community reverse-engineering history behind that libsigrok driver (Python/
  bluepy, Perl, and NodeJS decoders referenced there), and confirm BLE122's characteristic UUID is not
  published anywhere, only raw ATT handles (`gatttool -a 9 --char-write-req -n 0300 --listen`).

`crates/gw121/src/protocol.rs` ports libsigrok's bit offsets, mode/range tables and checksum as **facts** (protocol
constants, not copyrightable expression), independently written in idiomatic Rust, the same way m2k_cli ports
libm2k's LGPL formulas — except eev121gw.c is GPL-2.0-or-later, a stronger copyleft, so no code from it is
transcribed; only the numbers and bit positions it reports are. See `protocol.rs`'s module doc for the
byte-by-byte layout table.

## FEATURES.md is the source of truth

[FEATURES.md](FEATURES.md) lists every feature with its status, every known bug, and what is planned, with stable
IDs per area (`LIB`, `BLE`, `DMM`, `CLI`, `PY`, `SKILL`).

- **Read the relevant area before starting.** A "new" bug or idea is often already recorded.
- **Name IDs with a short slug when talking to the user**: `DMM-1 packet-format`, never a bare `DMM-1`. Commit
  messages, CHANGELOG and code `TODO`s keep the bare ID.
- **Update it in the same commit** as the code. New ids: `tpm work claim AREA` prints the next free one. Never
  renumber or reuse IDs.
- Status ✅ means verified on a real 121GW; 🧪 means recorded packets and/or unit tests only. Keep that honest,
  and say in the item what was checked how.

## CHANGELOG.md records every change

[CHANGELOG.md](CHANGELOG.md) is the history, FEATURES.md the current state; keep both.

- Every change a user would notice gets an entry under `## [Unreleased]` in the same commit: `### Added`,
  `### Changed`, `### Fixed`, `### Removed`. Short and user-facing, with the feature ID in parentheses.
- Also record new env vars. Pure refactors and typo fixes don't need an entry.

## Layout

Cargo workspace, edition 2024, one version for all crates (`[workspace.package]`), the same layout as the other
instrument tools (tpm CLAUDE.md "Code repos": tools are a library first):

- `crates/gw121` — the library, the typed API; everything that talks BLE or knows the packet format lives here.
  Errors are `gw121::Error` (thiserror), never `anyhow`.
  - `protocol.rs` — the 19-byte packet: byte offsets, mode/range scale tables, checksum, decoded into
    `Reading` (main/sub/bar, `Display` gives `-0.0021 V`, `rounded()` the display's digits) and `Flags`
    (global device state). No BLE or I/O here; pure parsing, unit-tested against recorded packets.
  - `reassemble.rs` — `Reassembler`: turns a stream of raw BLE indication byte chunks (not aligned to
    19-byte boundaries, and the link drops or truncates packets) into validated packets, resyncing on the next
    `0xF2` after anything that doesn't check out.
  - `ble.rs` — the BLE steps on `bluer`: open the adapter, scan, `find_meter` (exactly one 121GW), connect,
    and find the meter's data characteristic by its `indicate` GATT property (the UUID is not published; see
    the top of this file). Public so a caller can show progress between steps.
  - `meter.rs` — `Meter` (open/connect, `read`, `packets`, `disconnect`) and `Packets`, a `futures::Stream` of
    validated packets; `gw121::read` for connect-read-disconnect in one call.
  - `error.rs` — `Error`, one variant per case a caller may tell apart (not found, ambiguous, not seen,
    connect timeout, no packet, disconnected, bad address, `ParseError`, BlueZ).
- `crates/gw121_cli` — the thin `gw121` binary: `src/cli.rs` the command tree (clap derive) and handlers,
  `--json` output through `Out::print`, warnings through `Out::warn`, a spinner while scanning/connecting
  through `Out::spinner`; `src/parse.rs` duration parsing (`500ms`, `10s`); `src/state.rs` the last
  `gw121 scan`'s results, cached to disk for `--address` shell completion (a live scan is too slow for a Tab
  press); `src/complete.rs` dynamic shell completion, its Bash adapter copied from wire_weaver_cli /
  saleae_cli / m2k_cli (keep in sync); `build.rs` `GIT_SHA` and `BUILD_TIME` for `--version`. `anyhow` only
  here.
- `crates/gw121_py` — the `gw121` Python module (PyO3, abi3 for Python >= 3.9, built by maturin from
  `pyproject.toml`; `publish = false` on crates.io, it ships as a wheel): `scan()`, `read()`, `log()` (an
  iterator), `Meter`, `parse()`, classes `Packet`/`Reading`/`Flags`/`Device`, exceptions under `Gw121Error`
  mapped from `gw121::Error`. Calls run on one shared tokio runtime with the GIL released and check for Ctrl+C
  every 100 ms. `gw121.pyi` (type stubs and docstrings, shipped by maturin) must match `src/lib.rs`;
  `tests/test_smoke.py` is the offline pytest smoke test.
- `skills/gw121/SKILL.md` — the agent skill. Update it when commands or their output change.

## Commands

```sh
cargo build
cargo run -- scan
cargo run -- read
just lint                               # fmt check + clippy -D warnings on every crate
just test                               # cargo test --workspace + the Python smoke test; no BLE adapter or meter needed
just test-py                            # builds the Python module into /tmp/gw121_py-venv (uv + maturin), runs pytest
just install                            # puts `gw121` in ~/.cargo/bin
```

Before declaring a change done: `just lint` and `just test`. Tests never touch BLE —
everything there is checked against the packet bytes recorded in the sigrok wiki's own capture. Running
`scan`/`read`/`log` against a real meter needs: Bluetooth powered on this PC (`bluetoothctl power on`; `rfkill
unblock bluetooth` first if soft-blocked), and the meter in BLE mode (hold its **1ms PEAK** button until the
**BT** indicator lights). The meter is a piece of hardware next to this PC, not something this session can
touch directly — ask the user or the parent session (`tpm work say parent`) to put it in BLE mode or on a given
knob position when a change needs checking against it, then carry on with what can be tested without it. Say
in the report which commands were actually run against a real meter versus only against recorded packets.

## Hardware safety

Reading is all this tool does — there is no output or write path to the meter at all, so there is nothing here
like m2k_cli's or saleae_cli's "ask before driving an output" section. The one real risk is BLE state on this
PC: don't leave Bluetooth discovery running forever (`scan`'s `--timeout` bounds it), and don't change this
PC's `rfkill`/`bluetoothctl` pairing/trust state beyond powering the adapter on without asking first.

## Dependencies

Pure Rust only (tpm "Code repos" rule) **except** the BLE stack: `bluer` (and its alternative, `btleplug`) both
depend on the classic `dbus` crate, which binds the system `libdbus-1` C library via `libdbus-sys` — there is
no mature pure-Rust BlueZ GATT client to reach for instead, and the task that started this repo explicitly
named `bluer`/`btleplug` as the accepted exception over hand-rolling BlueZ's D-Bus API directly. `bluer` was
picked over `btleplug` because this tool only ever runs on Linux (the meter lives next to this PC) and `bluer`
gives direct access to GATT characteristic flags, which is how the data characteristic is found (its UUID
isn't published). Everything else: the library has thiserror, tokio and futures-util; the CLI clap and
clap_complete, indicatif, ctrlc, serde_json, dirs, anyhow; the Python crate pyo3. Check new dependencies with `cargo tree` for `-sys` crates and `cc`/`bindgen` build deps
beyond that one accepted exception.

## Publishing

Not published to crates.io or PyPI yet. When it is: crates `gw121` (library) and `gw121_cli` (binary
`gw121`), the Python wheel `gw121` from `crates/gw121_py` (maturin, abi3), repo
`github.com/romixlab/gw121_cli`, MIT or Apache-2.0. Public repo: nothing private in commits, docs or the skill
(no device serials, internal hosts, paths, ids). libsigrok's `eev121gw.c` is GPL-2.0-or-later: port facts
(bit positions, mode numbers, scale tables), never its code.

## Code conventions

- Errors: typed enums with `thiserror` in the library (`gw121::Error`, `protocol::ParseError`) since callers
  match on them (a corrupt packet is not a BLE disconnect, "no meter heard" not "two meters heard"); `anyhow`
  with `.context(...)` only in the CLI; in Python each `Error` variant maps to a `Gw121Error` subclass
  (`err()` in `gw121_py`). No `unwrap`/`expect` on data from the device.
- The CLI stays thin: new behaviour goes into the library first, then the CLI and the Python module call it.
- Every command supports `--json`; keep text output short and line-oriented (agents read it), units spelled
  out (`-0.0021 V`, `24.0 degC`).

## Tests

- Pure logic (packet parsing, reassembly, duration/reading formatting): Rust unit tests next to the code, all
  against the three packets and the BLE indication framing from the sigrok wiki's own `gatttool` capture
  (checksums verified by hand before trusting them as fixtures) — see `protocol.rs` and `reassemble.rs`.
- The Python module: `crates/gw121_py/tests/test_smoke.py` (pytest, offline: `parse()` on the same recorded
  packets, error mapping). Anything needing the meter is checked by hand and recorded in FEATURES.md.
- A bug fix starts with a failing test when the bug can be reproduced in one; a new mode/range combination
  gets a recorded packet as a fixture, not a hand-computed one, whenever a real capture is available.

## Commits

Conventional Commits with a scope: `feat(ble): ...`, `fix(protocol): ...`, scopes `lib`, `protocol`, `reassemble`,
`ble`, `cli`, `py`, `skill`, `build`. Short imperative summary, blank line, body with what and why; reference feature
IDs.

Commit on your own initiative on a session branch (`tpm work new SLUG`), each finished step one commit with
FEATURES.md and CHANGELOG.md updated in it. When the work is done and the user agrees, `tpm land` puts it on
main.

## Versions

Landing bumps the version, not each work commit (tpm CLAUDE.md "Landing bumps the version"): session commits add
CHANGELOG entries under `[Unreleased]` without bumping; `tpm land` makes the `release: x.y.z` commit.
`gw121 --version` prints version, git SHA and build time (`build.rs`, no extra crates).
