# gw121

Read an EEVblog 121GW multimeter over Bluetooth LE: a Rust library, a CLI and agent skill on top, and a Python
module for notebooks and test benches.

| crate | what |
|---|---|
| [`crates/gw121`](crates/gw121) | the library: scan, connect, typed `Packet`/`Reading`/`Flags`, a stream of packets, errors as an enum |
| [`crates/gw121_cli`](crates/gw121_cli) | the `gw121` command line tool |
| [`crates/gw121_py`](crates/gw121_py) | the `gw121` Python module (PyO3 + maturin) |

```sh
cargo install --path crates/gw121_cli       # (in a checkout with tpm: `just install`)
gw121 scan                  # find the meter: hold its 1ms PEAK button until BT lights
gw121 read                  # one reading: main display, sub display, bargraph, flags
gw121 log --for 1m --csv out.csv --every 1s
```

Mode and range live on the meter's own rotary knob; this tool only ever reads them, never sets them.

## Python

```sh
uv pip install ./crates/gw121_py       # in a uv venv; builds with maturin, needs Rust and libdbus-1 headers
uv add ./crates/gw121_py               # or as a dependency of a uv project
```

```python
import gw121

p = gw121.read()                        # scans for the one 121GW nearby, reads, disconnects
print(p.main, p.sub, p.flags.hold)      # -0.0021 V 24.0 degC False

for p in gw121.log(duration=60, every=1):          # one packet a second for a minute
    print(p.time, p.main.value, p.main.unit)

with gw121.Meter("88:6B:0F:73:00:01") as m:        # stay connected between reads
    readings = [m.read().main.value for _ in range(10)]
```

Errors are `gw121.Gw121Error` subclasses (`NotFoundError`, `ConnectError`, `DisconnectedError`, ...); type stubs
ship with the module.

## Rust

```rust
let meter = gw121::Meter::open(None, Duration::from_secs(10)).await?;
println!("{}", meter.read(Duration::from_secs(5)).await?.main()?); // -0.0021 V
let mut packets = meter.packets().await?;                           // a futures Stream
meter.disconnect().await?;
```

See [AGENTS.md](AGENTS.md) for how it works and where the packet format comes from, [FEATURES.md](FEATURES.md)
for what is implemented and verified, and [skills/gw121/SKILL.md](skills/gw121/SKILL.md) for the agent skill.

Licensed under MIT OR Apache-2.0.
