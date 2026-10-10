//! Read an EEVblog 121GW multimeter over Bluetooth LE: its main and secondary displays, the bargraph and its
//! flags (auto-range, hold, relative, AC/DC, battery). Mode and range live on the meter's knob and are only
//! ever read, never set.
//!
//! ```no_run
//! # async fn demo() -> gw121::Result<()> {
//! use futures_util::StreamExt;
//! use std::time::Duration;
//!
//! let meter = gw121::Meter::open(None, Duration::from_secs(10)).await?; // scans for the one 121GW nearby
//! let packet = meter.read(Duration::from_secs(5)).await?;
//! println!("{}", packet.main()?); // "-0.0021 V"
//! let mut packets = meter.packets().await?;
//! while let Some(packet) = packets.next().await {
//!     println!("{}", packet?.main()?);
//! }
//! meter.disconnect().await?;
//! # Ok(()) }
//! ```
//!
//! The packet format (ported from libsigrok, see AGENTS.md) is in [`protocol`], independent of BLE; reassembling
//! packets out of fragmented BLE indications in [`Reassembler`]; the BLE steps in [`ble`].

pub mod ble;
mod error;
mod meter;
pub mod protocol;
mod reassemble;

pub use ble::Found;
pub use bluer::{Adapter, Address};
pub use error::{Error, Result, parse_address};
pub use meter::{Meter, Packets, read};
pub use protocol::{Flags, PACKET_SIZE, Packet, ParseError, Quantity, Reading, Unit};
pub use reassemble::Reassembler;
