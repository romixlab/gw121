//! `gw121`: read an EEVblog 121GW multimeter over Bluetooth LE.
//!
//! The command tree and handlers live in `cli.rs`; the BLE transport in `ble.rs`; the packet format (ported
//! from libsigrok, see AGENTS.md) in `protocol.rs`; reassembling packets out of fragmented BLE indications in
//! `reassemble.rs`.

mod ble;
mod cli;
mod complete;
mod parse;
mod protocol;
mod reassemble;
mod state;

fn main() {
    cli::main()
}
