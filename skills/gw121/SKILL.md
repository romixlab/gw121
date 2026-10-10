---
name: gw121
description: Read an EEVblog 121GW multimeter's live display over Bluetooth LE with the `gw121` CLI: scan, read one value, or log continuously. Use when the user wants a voltage/current/resistance/temperature/frequency/capacitance reading from a 121GW, wants to log a measurement over time, or asks what the meter is showing right now.
---

# EEVblog 121GW multimeter readings

The `gw121` CLI (`just install` or `cargo install --path crates/gw121_cli` in this repo, or `cargo install gw121_cli` once published; source:
https://github.com/romixlab/gw121_cli) reads the 121GW's main display, secondary display, bargraph and flags
(auto-range, hold, relative, AC/DC, battery) over Bluetooth LE. **Mode and range are chosen on the meter's own
rotary knob; this tool only ever reads them, never sets them** — there is nothing here that can change what the
meter measures or drive anything.

Add `--json` to any command for machine-readable output (one object per line; errors become `{"error": "..."}`
with exit code 1).

## 0. Ask the user before connecting

You cannot see the meter or touch its buttons. Ask, in one message, only what you don't already know:

- **Is the meter in BLE mode?** Hold its **1ms PEAK** button until the **BT** indicator lights on the display.
  It stays that way until powered off or the connection is idle long enough that it sleeps.
- **What is the knob set to, and what's being measured?** (e.g. "DCV, probes on the two header pins" or
  "resistance, nothing connected"). You read whatever the knob is set to; you cannot change it.
- **Is Bluetooth on on this machine?** `rfkill list bluetooth` (unblock with `rfkill unblock bluetooth` if
  soft-blocked) and `bluetoothctl show` (power on with `bluetoothctl power on` if `Powered: no`) are safe to
  run and fix yourself without asking — they only affect this PC's radio, not the meter.

## 1. Find it, then read or log

```sh
gw121 scan                                    # lists nearby 121GW meters: address, name, RSSI
gw121 read                                     # scans if needed, connects, prints one reading, disconnects
gw121 read --address 88:6B:0F:73:00:01         # skip the scan when you already know the address
gw121 log --for 1m --every 1s                  # print at most once a second for a minute, then stop
gw121 log --csv trace.csv                      # also append every line to a CSV file
```

Without `--address`, `read`/`log` scan for a few seconds and use the one 121GW found; they error out (naming
the addresses) if more than one answers, or if none does — in which case re-check BLE mode and range before
trying again.

`log` runs until Ctrl+C or `--for DURATION`; without `--every`, it prints every packet the meter sends (about
twice a second, per the sigrok wiki). Pass both `--csv` and `--json`/plain text at once: the CSV always gets a
row, stdout gets a line, both throttled together by `--every`.

## 2. Reading the output

```
main: -0.0021 V DC
sub:  24.0 degC
bar:  0.0
```

- `main` is the large readout: value, unit, and `AC`/`DC`/`AC+DC` when the mode has a sign (voltage, current,
  temperature, power — resistance, capacitance, diode and continuity never show a sign). `OVER`/`-OVER`
  instead of a number means the input is out of range for the current (possibly auto-) range.
- `sub` is the smaller secondary readout — very often the meter's internal temperature, but can be a second
  live measurement (e.g. battery voltage, a diode test's configured voltage) depending on what the user set
  it to on the meter. Missing entirely (no `sub:` line) is normal in some modes/menus, not an error.
- `bar` is the analog bargraph as a plain number, same scale family as `main`'s active range; missing when
  the firmware blanks it for the current mode.
- A trailing line of tags (`AUTO`, `HOLD`, `REL`, `LOW-BATT`) reports the meter's own flags. No line means
  none of those are set.

`--json`'s `main`/`sub` objects have `value` (a number, or the string `"inf"`/`"-inf"` on overload — JSON has
no infinity), `unit`, `quantity`, `digits` (display precision, can be negative: round to tens/hundreds/...),
`ac`, `dc`, `overload`; `bar` has just `value`/`digits`; `flags` has every flag as a bool.

## 3. Report to the user

State the value plainly with its unit and mode (e.g. "main display reads -0.0021 V DC, auto-range, nothing
else flagged"). If `log` was used, summarize the range/trend over the run rather than every line, and mention
the CSV path if one was written.

## From Python

For a notebook or a test script, the `gw121` Python module does the same (`pip install ./crates/gw121_py` in
this repo; needs Rust):

```python
import gw121
p = gw121.read()                            # p.main.value, p.main.unit, str(p.sub), p.flags.hold
for p in gw121.log(duration=60, every=1):   # disconnects when the loop ends
    ...
```

Errors are `gw121.Gw121Error` subclasses: `NotFoundError` (no meter heard: ask about BLE mode),
`DisconnectedError` (meter switched off or asleep), `NoPacketError`, `ConnectError`.

## Limits

- BLE communication with this meter is reported to be slow (about two samples a second) and occasionally
  flaky (dropped or truncated packets); `gw121` resyncs past bad data automatically, so an occasional gap in a
  `log` run is expected, not a bug to chase.
- A handful of packet fields have no confirmed meaning even in the upstream decoder this is built on (an
  "unknown" status bit, the memory/min-max bitfields, continuity's beeper threshold): `gw121` surfaces the
  flags it is confident about and leaves the rest out rather than guessing (see FEATURES.md, `DMM-1`).
- Nothing here has been run against a real 121GW yet (see FEATURES.md for exactly what is and isn't verified
  so far) — treat a first real session as also testing the tool, and say so if something looks off.
