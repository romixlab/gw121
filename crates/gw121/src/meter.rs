//! A connected meter ([`Meter`]) and its stream of validated packets ([`Packets`]).

use crate::ble;
use crate::protocol::Packet;
use crate::reassemble::Reassembler;
use crate::{Error, Result};
use bluer::gatt::remote::Characteristic;
use bluer::{Adapter, Address, Device};
use futures_util::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// How long [`Meter::disconnect`] waits for BlueZ before giving up on a clean disconnect.
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// A 121GW connected over BLE, with its data characteristic found.
///
/// Disconnect with [`Meter::disconnect`] when done: a meter left connected stops advertising, so the next scan
/// would not hear it. Dropping a `Meter` inside a tokio runtime disconnects in the background as a fallback.
#[derive(Debug)]
pub struct Meter {
    address: Address,
    device: Device,
    characteristic: Characteristic,
    disconnected: bool,
}

impl Meter {
    /// Connects to the meter at `address`, or, without one, scans for the one 121GW nearby. `timeout` bounds
    /// the scan and the connect each.
    pub async fn open(address: Option<Address>, timeout: Duration) -> Result<Meter> {
        let adapter = ble::adapter().await?;
        let address = match address {
            Some(a) => a,
            None => ble::find_meter(&adapter, timeout).await?.address,
        };
        Self::connect(&adapter, address, timeout).await
    }

    /// Connects to `address` and finds its data characteristic, all within `timeout`.
    ///
    /// BlueZ keeps a meter it saw earlier as a device object and its connect waits on its own, much longer
    /// timeout when the meter is off: hence one bound on the whole connect + GATT lookup.
    pub async fn connect(adapter: &Adapter, address: Address, timeout: Duration) -> Result<Meter> {
        let connect = async {
            let device = ble::connect(adapter, address, timeout).await?;
            let characteristic = ble::find_data_characteristic(&device).await?;
            Ok::<_, Error>((device, characteristic))
        };
        let (device, characteristic) = tokio::time::timeout(timeout, connect)
            .await
            .map_err(|_| Error::ConnectTimeout { address, timeout })??;
        Ok(Meter {
            address,
            device,
            characteristic,
            disconnected: false,
        })
    }

    pub fn address(&self) -> Address {
        self.address
    }

    /// Subscribes to the meter's indications: about two packets a second until the meter goes away.
    pub async fn packets(&self) -> Result<Packets> {
        Ok(Packets {
            address: self.address,
            inner: Some(ble::notify_stream(&self.characteristic).await?),
            reassembler: Reassembler::new(),
        })
    }

    /// The next valid packet, waiting at most `timeout` for it.
    pub async fn read(&self, timeout: Duration) -> Result<Packet> {
        let mut packets = self.packets().await?;
        tokio::time::timeout(timeout, packets.next_packet())
            .await
            .map_err(|_| Error::NoPacket {
                address: self.address,
                timeout,
            })?
    }

    /// Disconnects, waiting a few seconds at most for BlueZ to confirm.
    pub async fn disconnect(mut self) -> Result<()> {
        self.disconnected = true;
        match tokio::time::timeout(DISCONNECT_TIMEOUT, self.device.disconnect()).await {
            Ok(r) => Ok(r?),
            Err(_) => Ok(()),
        }
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        if self.disconnected {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let device = self.device.clone();
            runtime.spawn(async move {
                let _ = tokio::time::timeout(DISCONNECT_TIMEOUT, device.disconnect()).await;
            });
        }
    }
}

/// Validated packets from a [`Meter`], reassembled out of its BLE indications (see [`Reassembler`]). Yields
/// [`Error::Disconnected`] once when the meter goes away, then ends.
pub struct Packets {
    address: Address,
    inner: Option<ble::DataStream>,
    reassembler: Reassembler,
}

impl Packets {
    /// The next packet, or [`Error::Disconnected`] when the meter went away.
    pub async fn next_packet(&mut self) -> Result<Packet> {
        self.next()
            .await
            .unwrap_or(Err(Error::Disconnected(self.address)))
    }
}

impl Stream for Packets {
    type Item = Result<Packet>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(buf) = this.reassembler.next_packet() {
                return Poll::Ready(Some(Packet::parse(buf).map_err(Error::from)));
            }
            let Some(inner) = this.inner.as_mut() else {
                return Poll::Ready(None);
            };
            match inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(chunk)) => this.reassembler.push(&chunk),
                Poll::Ready(None) => {
                    this.inner = None;
                    return Poll::Ready(Some(Err(Error::Disconnected(this.address))));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Connects (scanning when `address` is `None`), reads one packet and disconnects; `timeout` bounds the scan,
/// the connect and the wait for the packet each.
pub async fn read(address: Option<Address>, timeout: Duration) -> Result<Packet> {
    let meter = Meter::open(address, timeout).await?;
    let packet = meter.read(timeout).await;
    let _ = meter.disconnect().await;
    packet
}
