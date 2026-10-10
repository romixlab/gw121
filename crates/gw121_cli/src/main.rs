//! `gw121`: read an EEVblog 121GW multimeter over Bluetooth LE.
//!
//! A thin binary on the `gw121` library crate (BLE, packet format, reassembly): the command tree and handlers
//! live in `cli.rs`, shell completion in `complete.rs`, the last scan's cache in `state.rs`.

mod cli;
mod complete;
mod parse;
mod state;

fn main() {
    cli::main()
}
