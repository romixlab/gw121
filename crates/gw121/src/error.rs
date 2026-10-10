//! The library's error type: one variant per thing a caller may want to tell apart (no meter found, more than
//! one, a meter that went away, a corrupt packet, BlueZ itself failing).

use crate::protocol::ParseError;
use bluer::Address;
use std::time::Duration;

/// Everything that can go wrong finding, connecting to or reading from a 121GW.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// BlueZ (over D-Bus) failed: no adapter, Bluetooth off or blocked, permission denied, ...
    #[error(transparent)]
    Bluetooth(#[from] bluer::Error),
    /// A scan for a 121GW ended without hearing one.
    #[error(
        "no 121GW seen in {0:?}; hold its 1ms PEAK button until the BT indicator lights, or give its address"
    )]
    NotFound(Duration),
    /// A scan heard more than one 121GW: which one to use is the caller's choice.
    #[error("more than one 121GW nearby ({}); give the address of one", list(.0))]
    Ambiguous(Vec<Address>),
    /// The given address was not heard while scanning, so BlueZ cannot connect to it.
    #[error(
        "{address} was not seen while scanning for {timeout:?}; is the meter in BLE mode and in range?"
    )]
    NotSeen { address: Address, timeout: Duration },
    /// Connecting and finding the data characteristic took longer than the timeout.
    #[error(
        "could not connect to {address} in {timeout:?}; is the meter on, in BLE mode and in range?"
    )]
    ConnectTimeout { address: Address, timeout: Duration },
    /// The device has no GATT characteristic with the indicate property.
    #[error(
        "no GATT characteristic with the indicate property on {0}; not a 121GW, or its GATT table changed"
    )]
    NoDataCharacteristic(Address),
    /// Connected, but no valid packet arrived in time.
    #[error("no valid packet from {address} in {timeout:?}")]
    NoPacket { address: Address, timeout: Duration },
    /// The meter went away mid-stream: switched off, out of range, or it fell asleep.
    #[error("{0} disconnected (meter off or out of range)")]
    Disconnected(Address),
    /// Text that is not a BLE address.
    #[error("`{0}` is not a BLE address (expected 88:6B:0F:73:00:01)")]
    BadAddress(String),
    /// A packet that failed validation, or a field no known mode/range table covers.
    #[error(transparent)]
    Packet(#[from] ParseError),
}

/// `Result` with this crate's [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

fn list(addresses: &[Address]) -> String {
    addresses
        .iter()
        .map(Address::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parses `88:6B:0F:73:00:01`.
pub fn parse_address(text: &str) -> Result<Address> {
    text.trim()
        .parse()
        .map_err(|_| Error::BadAddress(text.to_string()))
}
