# gw121_cli

Rust CLI and agent skill for the EEVblog 121GW multimeter over Bluetooth LE.

```sh
cargo install --path .
gw121 scan                  # find the meter: hold its 1ms PEAK button until BT lights
gw121 read                  # one reading: main display, sub display, bargraph, flags
gw121 log --for 1m --csv out.csv --every 1s
```

Mode and range live on the meter's own rotary knob; this tool only ever reads them, never sets them.

See [AGENTS.md](AGENTS.md) for how it works and where the packet format comes from, [FEATURES.md](FEATURES.md)
for what is implemented and verified, and [skills/gw121/SKILL.md](skills/gw121/SKILL.md) for the agent skill.

Licensed under MIT OR Apache-2.0.
