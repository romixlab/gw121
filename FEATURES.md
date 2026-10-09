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

Real-device checks so far: no 121GW yet (the meter is next to this PC but was not in BLE mode while this was
built; `tpm work say parent` was used to ask for it). `gw121 --version`, `--help` and shell completion were run
directly; `gw121 scan --all` was run against this PC's real Bluetooth adapter and found five real nearby BLE
devices (a smartwatch, two TVs), confirming the adapter-open/discover/name/RSSI path works against real BlueZ
— just not yet against the meter itself (connect and GATT characteristic lookup are untested on real BLE).

## BLE transport (`BLE`)

- 🧪 **BLE-1 Scan, connect, subscribe**: `ble.rs` uses `bluer` (BlueZ over D-Bus; AGENTS.md has why this and
  not a hand-rolled D-Bus client). Scans with a name filter, connects, and finds the meter's data
  characteristic at runtime by its `indicate` GATT property rather than a hardcoded UUID — the 121GW's
  characteristic UUID is not published, only the raw ATT handles the sigrok wiki's `gatttool` capture used
  (handle 8 for data, handle 9's CCCD for the subscribe), which BlueZ's D-Bus API does not expose directly.
  Scanning was run against this PC's real Bluetooth adapter (`gw121 scan --all`, found five real nearby
  devices); connect and GATT characteristic lookup are untested on real BLE so far — no 121GW was in BLE mode
  yet while building this.
- 🧪 **BLE-2 Reassembly**: `reassemble.rs`'s `Reassembler` resyncs 19-byte packets out of a stream of raw BLE
  indication bytes (not aligned to packet boundaries, and the link is reported to drop or truncate packets).
  Unit-tested against the exact indication bytes from the sigrok wiki's own `gatttool` capture, both the clean
  case and injected corruption; not yet exercised against a live, possibly flakier BLE link.

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
- 🧪 **CLI-4 scan**: lists nearby 121GW meters (or `--all` BLE devices) with address, name, RSSI; saves the
  result for `--address` completion. Ran `gw121 scan --all` against this PC's real adapter and got five real
  nearby devices; the 121GW-name filter and the completion cache it saves are not yet checked against a real
  meter.
- 🧪 **CLI-5 read**: connects, subscribes, prints the first valid packet's main/sub/bar readings and flags,
  then disconnects. `--address` to skip auto-discovery, `--timeout` for the whole scan+connect+first-packet
  budget. Not yet run against a real meter.
- 🧪 **CLI-6 log**: connects and prints readings continuously until Ctrl+C or `--for`; `--every` throttles how
  often a line is emitted (and how often a CSV row is written) to at most once per interval; `--csv FILE`
  appends rows alongside the normal text/JSON output. Not yet run against a real meter.

## Agent skill (`SKILL`)

- 🧪 **SKILL-1 skills/gw121/SKILL.md**: what to ask about the meter's BLE mode and knob position, read/log
  recipes, reading the output, what mode/range numbers mean. Not yet used on a real debugging session.
- 📋 **SKILL-2 Skill tree**: usage signals and skilltree entries for the `gw121` commands (P2618 task).

## Dropped and superseded

(none yet)
