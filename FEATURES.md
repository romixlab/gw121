# gw121_cli features and roadmap

This file is the single source of truth for what gw121_cli does, what is broken and what is planned.
[CHANGELOG.md](CHANGELOG.md) records what changed and when; this file records the current state.

## How to use this file

- **Status**: ✅ done (verified on a real 121GW) · 🧪 done, verified against recorded packets and/or unit
  tests only · 🚧 partial (the note says what is missing) · 🐛 known bug · 📋 planned · 💡 idea ·
  ⛔ blocked (on what) · 🔍 needs a check.
- **IDs** (`DMM-1`) are stable: never renumber or reuse one. New ids come from `tpm work claim AREA`. Use the
  ID in commit messages, CHANGELOG entries and code `TODO`s (`// TODO(DMM-1): ...`); name it with a short slug
  when talking to people (`DMM-1 packet-format`).
- When finishing work, update the item in the same commit. Obsolete items go to *Dropped and superseded*.

Real-device checks so far (10 Oct 2026, a real 121GW in BLE mode next to this PC, knob on DCV, probes on a
3.3 V rail): `gw121 scan`, `read`, `--json read` and `log --for 4s` (CLI), and `gw121.scan()`, `read()`,
`log(duration=4, every=1)`, `Meter` with `read()`/`log()`/`close()`, and Ctrl+C during `log()` (Python), all
decoded 3.3253 V DC main, ~25 degC sub, bar 3.2, AUTO; the meter could be read again right after each, so
disconnecting works. Not yet checked on the real meter: other knob positions, overload, HOLD/REL, a meter
switched off mid-`log`. `--version`, `--help` and shell completion were run directly.

## Library (`LIB`)

- ✅ **LIB-1 Library-first split**: a Cargo workspace: `crates/gw121` (the library: `Meter` open/connect,
  `read`, `packets` as a `futures::Stream` of validated packets, `disconnect`; `gw121::read`; `ble::scan`,
  `ble::find_meter`; typed `Packet`/`Reading`/`Flags`; `gw121::Error` via thiserror with one variant per
  case), `crates/gw121_cli` (the same `gw121` commands, flags, JSON and completions, on the library) and
  `crates/gw121_py` (PY-1). Unit tests moved with the code and pass; the CLI and the Python module were run against a
  real 121GW after the split (see the top of this file).

## BLE transport (`BLE`)

- ✅ **BLE-1 Scan, connect, subscribe**: `ble.rs` uses `bluer` (BlueZ over D-Bus; AGENTS.md has why this and
  not a hand-rolled D-Bus client). Scans with a name filter, connects, and finds the meter's data
  characteristic at runtime by its `indicate` GATT property rather than a hardcoded UUID — the 121GW's
  characteristic UUID is not published, only the raw ATT handles the sigrok wiki's `gatttool` capture used
  (handle 8 for data, handle 9's CCCD for the subscribe), which BlueZ's D-Bus API does not expose directly.
  Scan, connect, finding the indicate characteristic, subscribing and disconnecting all worked on a real
  121GW (10 Oct 2026): the meter has exactly one indicate characteristic, as expected.
- 🧪 **BLE-2 Reassembly**: `reassemble.rs`'s `Reassembler` resyncs 19-byte packets out of a stream of raw BLE
  indication bytes (not aligned to packet boundaries, and the link is reported to drop or truncate packets).
  Unit-tested against the exact indication bytes from the sigrok wiki's own `gatttool` capture, both the clean
  case and injected corruption; on a real meter it produced clean packets for a few seconds of `log`; not yet exercised on a long, flaky
  link.

## Packet format (`DMM`)

- 🧪 **DMM-1 121GW 19-byte packet parser**: `protocol.rs`. Ported as facts (bit offsets, mode numbers,
  mode/range scale tables), not copied code, from libsigrok's GPL-2.0-or-later `eev121gw` DMM driver
  (`src/dmm/eev121gw.c`), the only published, byte-accurate decoder for this format — see AGENTS.md for the
  full provenance chain from the sigrok wiki down to that source file. Covers the main and sub displays
  (voltage, current, power, resistance, capacitance, temperature, frequency, period, duty cycle, continuity,
  diode, gain/dBm) and the bargraph. Unit-tested against three real packets (checksums verified by hand) from
  the sigrok wiki's BLE capture, decoded to -0.0021 V DC / -0.0001 V DC / +0.0031 V DC on the main display and
  24.0 / 24.0 / 23.9 °C on the sub display — plausible for "meter idling on a DC-volts range, probes open,
  temperature sub-display", which is the best available check without a meter in hand. Checksum and reserved-
  bit validation match libsigrok's `sr_eev121gw_packet_valid` exactly, including its one deliberately disabled
  check (bit 5 of the bargraph value byte, seen set on a real meter in FREQ/OHM per upstream's comment).
  Known gaps, all inherited from upstream's own TODOs since no further reverse-engineering was done here: the
  "unknown" icon-status-2 bit (possibly a 20 mA current-loop indicator), the exact meaning of the memory and
  min/max/avg bitfields, and the continuity mode's open/closed beeper state (not present in the packet at
  all). `LowZ` mode's forced low-pass flag is not folded into the global `Flags` (a deliberate simplification:
  `Flags` is read independently of which display's mode produced a packet, see protocol.rs doc comment).
- 🧪 **DMM-2 Global flags**: auto-range, auto-power-off, low battery, Bluetooth, hold, relative, test, memory,
  1 kHz low-pass, 1 ms peak — from the packet's three "icon" bytes, independent of which display is read.
  Verified the same way as DMM-1 (the three recorded packets decode to auto-power-off on, nothing else set,
  matching an idle meter with default settings).

## CLI (`CLI`)

- ✅ **CLI-1 Version**: `--version` prints version, git SHA (`-dirty`) and build time (`build.rs`). Run
  directly: `gw121 0.1.0 (e4eaa86-dirty, built ...)`.
- ✅ **CLI-2 Shell completion**: dynamic, `source <(COMPLETE=bash gw121)` (zsh, fish, elvish, powershell);
  `--address` candidates come from the last `gw121 scan`'s cache (`state.rs`), not a live BLE scan (too slow
  for a Tab press); the Bash adapter is copied from saleae_cli/m2k_cli/wire_weaver_cli (keep in sync). Ran the
  registration script directly (`COMPLETE=bash gw121`); the candidate functions were not exercised against a
  real cached scan.
- ✅ **CLI-3 JSON output**: `--json` on every command (one object per line), errors as `{"error": ...}` with
  exit code 1; text output is short and line-oriented.
- ✅ **CLI-4 scan**: lists nearby 121GW meters (or `--all` BLE devices) with address, name, RSSI; saves the
  result for `--address` completion. Ran `gw121 scan --all` against this PC's real adapter and got five real
  nearby devices; `gw121 scan` found the real 121GW by name (`88:6B:...  121GW  -78 dBm`).
- ✅ **CLI-5 read**: connects, subscribes, prints the first valid packet's main/sub/bar readings and flags,
  then disconnects. `--address` to skip auto-discovery, `--timeout` for the whole scan+connect+first-packet
  budget. Ran on a real 121GW, text and `--json`: `main: 3.3253 V DC`, `sub: 25.3 degC`, `bar: 3.2`, `AUTO`.
- ✅ **CLI-6 log**: connects and prints readings continuously until Ctrl+C or `--for`; `--every` throttles how
  often a line is emitted (and how often a CSV row is written) to at most once per interval; `--csv FILE`
  appends rows alongside the normal text/JSON output. `log --for 4s` ran on a real 121GW and stopped on time;
  `--csv` and `--every` not yet run on the meter.

## Python (`PY`)

- ✅ **PY-1 Python module `gw121`**: `crates/gw121_py`, PyO3 (abi3, Python >= 3.9) built with maturin:
  `scan()`, `read()`, `log()` (an iterator of packets with `duration`/`every`/`timeout`, a context manager
  that disconnects), `Meter` (stay connected: `read()`, `log()`, `close()`, `with`), `parse(bytes)` offline;
  `Packet` (`main`, `sub`, `bar`, `flags`, `time`, `raw`), `Reading` (`value` rounded to the display's digits,
  `unit`, `quantity`, `str()` as displayed), exceptions under `Gw121Error` mapped from `gw121::Error`. Calls
  release the GIL and stop on Ctrl+C. Type stubs (`gw121.pyi`) ship in the wheel. `just test-py` builds it in
  a venv under /tmp and runs the offline pytest smoke test (recorded packets, error mapping). `scan()`,
  `read()`, `log(duration, every)`, `Meter` and Ctrl+C were run on a real 121GW (see the top of this file).

## Agent skill (`SKILL`)

- 🧪 **SKILL-1 skills/gw121/SKILL.md**: what to ask about the meter's BLE mode and knob position, read/log
  recipes, reading the output, what mode/range numbers mean. Not yet used on a real debugging session.
- 📋 **SKILL-2 Skill tree**: usage signals and skilltree entries for the `gw121` commands (P2618 task).

## Dropped and superseded

(none yet)
