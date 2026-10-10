//! BLE transport: find the meter, connect, and get a stream of raw bytes from its indicate characteristic.
//!
//! Uses `bluer` (the pure-Rust-API wrapper around BlueZ's D-Bus GATT interface), per the project's choice to
//! not hand-roll D-Bus calls. The meter's GATT characteristic UUID is not published (the sigrok wiki's
//! capture addresses it by raw ATT handle, `gatttool -a 9`/`-a 8`, which BlueZ's D-Bus API does not expose);
//! instead this discovers it at connect time as "the characteristic that supports indicate", which the 121GW
//! was observed to have exactly one of (AGENTS.md has the detail).
//!
//! [`crate::Meter`] wraps these steps; they are public for callers that want to show progress between them.

use crate::{Error, Result};
use bluer::gatt::remote::Characteristic;
use bluer::{Adapter, Address, Device, Session};
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use std::time::Duration;

/// What a 121GW advertises its name as, matched case-insensitively.
pub const METER_NAME: &str = "121gw";

/// A stream of raw BLE indication payloads, boxed because `Characteristic::notify`'s stream type isn't `Unpin`
/// (it is built from `pin_project`-based combinators internally) and callers need to hold onto it across
/// `.await` points (`tokio::time::timeout(..., stream.next())`).
pub type DataStream = Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;

/// A device seen while scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub address: Address,
    pub name: Option<String>,
    pub rssi: Option<i16>,
    /// Connected to this PC already: a connected meter does not advertise, so it has no RSSI.
    pub connected: bool,
}

/// Opens a session and the default adapter, powering it on if needed.
pub async fn adapter() -> Result<Adapter> {
    let session = Session::new().await?;
    let adapter = session.default_adapter().await?;
    if !adapter.is_powered().await? {
        adapter.set_powered(true).await?;
    }
    Ok(adapter)
}

/// Scans for `timeout`, returning every device whose name contains `name_filter` (case-insensitive), or every
/// device when `name_filter` is empty. Newest-seen first. With `first`, stops at the first match.
pub async fn scan(
    adapter: &Adapter,
    timeout: Duration,
    name_filter: &str,
    first: bool,
) -> Result<Vec<Found>> {
    let filter = name_filter.to_lowercase();
    let mut events = adapter.discover_devices().await?;
    let mut found = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let Ok(Some(event)) = tokio::time::timeout(remaining, events.next()).await else {
            break;
        };
        let bluer::AdapterEvent::DeviceAdded(address) = event else {
            continue;
        };
        let device = adapter.device(address)?;
        let name = device.name().await.ok().flatten();
        if !filter.is_empty()
            && !name
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&filter)
        {
            continue;
        }
        let rssi = device.rssi().await.ok().flatten();
        found.retain(|f: &Found| f.address != address);
        found.insert(
            0,
            Found {
                address,
                name,
                rssi,
                connected: false,
            },
        );
        if first {
            break;
        }
    }
    // A device BlueZ remembers is reported at once, before it is heard: read RSSI again now that the scan
    // ran. Still none means it was not heard at all (off or out of range).
    for f in &mut found {
        if let Ok(device) = adapter.device(f.address) {
            f.rssi = device.rssi().await.ok().flatten();
            f.connected = device.is_connected().await.unwrap_or(false);
        }
    }
    Ok(found)
}

/// Scans for up to `timeout` and returns the one 121GW heard: [`Error::NotFound`] when none answers,
/// [`Error::Ambiguous`] when several do.
pub async fn find_meter(adapter: &Adapter, timeout: Duration) -> Result<Found> {
    let mut found = scan(adapter, timeout, METER_NAME, true).await?;
    match found.len() {
        0 => Err(Error::NotFound(timeout)),
        1 => Ok(found.remove(0)),
        _ => Err(Error::Ambiguous(found.iter().map(|f| f.address).collect())),
    }
}

/// Makes sure BlueZ has a `Device1` object for `address` (it needs to have seen it via discovery at least
/// once), then connects.
pub async fn connect(adapter: &Adapter, address: Address, timeout: Duration) -> Result<Device> {
    // `adapter.device()` gives a handle for any address; whether BlueZ knows it shows in its device list.
    if !adapter.device_addresses().await?.contains(&address) {
        let mut events = adapter.discover_devices().await?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(Error::NotSeen { address, timeout });
            }
            match tokio::time::timeout(remaining, events.next()).await {
                Ok(Some(bluer::AdapterEvent::DeviceAdded(a))) if a == address => break,
                Ok(Some(_)) => continue,
                _ => return Err(Error::NotSeen { address, timeout }),
            }
        }
    }
    let device = adapter.device(address)?;
    if !device.is_connected().await.unwrap_or(false) {
        device.connect().await?;
    }
    Ok(device)
}

/// The GATT characteristic to read from: the device's only characteristic with the indicate property.
pub async fn find_data_characteristic(device: &Device) -> Result<Characteristic> {
    for service in device.services().await? {
        for characteristic in service.characteristics().await? {
            if characteristic.flags().await?.indicate {
                return Ok(characteristic);
            }
        }
    }
    Err(Error::NoDataCharacteristic(device.address()))
}

/// Subscribes and returns a stream of raw byte chunks, one per BLE indication. Feed these to
/// [`crate::Reassembler`] to get packets, or use [`crate::Meter::packets`] which does.
pub async fn notify_stream(characteristic: &Characteristic) -> Result<DataStream> {
    Ok(Box::pin(characteristic.notify().await?))
}
