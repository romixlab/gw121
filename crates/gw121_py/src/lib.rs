//! Python module for the EEVblog 121GW multimeter over Bluetooth LE, on the `gw121` crate.
//!
//! ```python
//! import gw121
//!
//! print(gw121.read().main)                     # scans for the one 121GW nearby: -0.0021 V
//! for p in gw121.log(duration=10, every=1):    # one packet a second for ten seconds
//!     print(p.time, p.main.value, p.main.unit)
//! with gw121.Meter("88:6B:0F:73:00:01") as m:  # stay connected between reads
//!     m.read().sub
//! ```
//!
//! Calls block with the GIL released; Ctrl+C interrupts them (checked every 100 ms).

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use std::future::Future;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

create_exception!(
    gw121,
    Gw121Error,
    PyException,
    "Base class of every gw121 error."
);
create_exception!(
    gw121,
    BluetoothError,
    Gw121Error,
    "BlueZ failed: no adapter, Bluetooth off or blocked, permission denied, ..."
);
create_exception!(
    gw121,
    NotFoundError,
    Gw121Error,
    "A scan heard no 121GW: is it on, in BLE mode (hold 1ms PEAK until BT lights) and in range?"
);
create_exception!(
    gw121,
    AmbiguousError,
    Gw121Error,
    "A scan heard more than one 121GW: pass the address of one."
);
create_exception!(
    gw121,
    ConnectError,
    Gw121Error,
    "Could not connect: the address was not heard, connecting timed out, or it is not a 121GW."
);
create_exception!(
    gw121,
    NoPacketError,
    Gw121Error,
    "Connected, but no valid packet arrived within the timeout."
);
create_exception!(
    gw121,
    DisconnectedError,
    Gw121Error,
    "The meter went away: switched off, out of range, or asleep."
);
create_exception!(
    gw121,
    PacketError,
    Gw121Error,
    "Bytes that are not a valid 121GW packet."
);
create_exception!(
    gw121,
    AddressError,
    Gw121Error,
    "Text that is not a BLE address."
);

fn err(e: gw121::Error) -> PyErr {
    use gw121::Error::*;
    let msg = e.to_string();
    match e {
        Bluetooth(_) => BluetoothError::new_err(msg),
        NotFound(_) => NotFoundError::new_err(msg),
        Ambiguous(_) => AmbiguousError::new_err(msg),
        NotSeen { .. } | ConnectTimeout { .. } | NoDataCharacteristic(_) => {
            ConnectError::new_err(msg)
        }
        NoPacket { .. } => NoPacketError::new_err(msg),
        Disconnected(_) => DisconnectedError::new_err(msg),
        BadAddress(_) => AddressError::new_err(msg),
        Packet(_) => PacketError::new_err(msg),
        _ => Gw121Error::new_err(msg),
    }
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("starting the tokio runtime")
    })
}

/// Runs `fut` to completion with the GIL released, checking for Ctrl+C (and other signals) every 100 ms.
fn wait<F: Future + Send>(py: Python<'_>, fut: F) -> PyResult<F::Output>
where
    F::Output: Send,
{
    const TICK: Duration = Duration::from_millis(100);
    let mut fut = std::pin::pin!(fut);
    loop {
        let polled = py.detach(|| {
            runtime().block_on(async { tokio::time::timeout(TICK, fut.as_mut()).await })
        });
        if let Ok(out) = polled {
            return Ok(out);
        }
        py.check_signals()?;
    }
}

fn seconds(s: f64, what: &str) -> PyResult<Duration> {
    Duration::try_from_secs_f64(s)
        .map_err(|_| PyValueError::new_err(format!("{what} must be a number of seconds >= 0")))
}

fn address(text: Option<&str>) -> PyResult<Option<gw121::Address>> {
    text.map(gw121::parse_address).transpose().map_err(err)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// One display's value: main, sub or the bargraph.
#[pyclass(module = "gw121", frozen, skip_from_py_object)]
#[derive(Clone)]
struct Reading {
    /// The value in `unit`, rounded to the digits the display shows; `inf`/`-inf` on overload.
    #[pyo3(get)]
    value: f64,
    /// Decimal places the display shows; negative for a range shown in tens, hundreds, ...
    #[pyo3(get)]
    digits: i32,
    /// `voltage`, `current`, `power`, `resistance`, `capacitance`, `temperature`, `frequency`, `period`,
    /// `dutycycle`, `continuity`, `gain`, `count` (the bargraph).
    #[pyo3(get)]
    quantity: &'static str,
    /// `V`, `A`, `VA`, `dBm`, `ohm`, `F`, `degC`, `degF`, `Hz`, `s`, `%` or `` (the bargraph).
    #[pyo3(get)]
    unit: &'static str,
    #[pyo3(get)]
    overload: bool,
    #[pyo3(get)]
    ac: bool,
    #[pyo3(get)]
    dc: bool,
    text: String,
}

impl From<gw121::Reading> for Reading {
    fn from(r: gw121::Reading) -> Self {
        Reading {
            value: r.rounded(),
            digits: r.digits,
            quantity: r.quantity.name(),
            unit: r.unit.symbol(),
            overload: r.overload,
            ac: r.ac,
            dc: r.dc,
            text: r.to_string(),
        }
    }
}

#[pymethods]
impl Reading {
    /// As the display shows it: `-0.0021 V`, `OVER V`.
    fn __str__(&self) -> &str {
        &self.text
    }

    fn __repr__(&self) -> String {
        format!("<Reading {}>", self.text)
    }

    fn __float__(&self) -> f64 {
        self.value
    }
}

/// The meter's own flags, not tied to one display.
#[pyclass(module = "gw121", frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct Flags {
    auto_range: bool,
    auto_power_off: bool,
    low_battery: bool,
    bluetooth: bool,
    hold: bool,
    relative: bool,
    test: bool,
    memory: bool,
    low_pass: bool,
    peak_1ms: bool,
}

#[pymethods]
impl Flags {
    fn __repr__(&self) -> String {
        let set: Vec<&str> = [
            ("auto_range", self.auto_range),
            ("auto_power_off", self.auto_power_off),
            ("low_battery", self.low_battery),
            ("bluetooth", self.bluetooth),
            ("hold", self.hold),
            ("relative", self.relative),
            ("test", self.test),
            ("memory", self.memory),
            ("low_pass", self.low_pass),
            ("peak_1ms", self.peak_1ms),
        ]
        .into_iter()
        .filter_map(|(name, on)| on.then_some(name))
        .collect();
        format!("<Flags {}>", set.join(" "))
    }
}

/// One packet from the meter: both displays, the bargraph and the flags at one moment.
#[pyclass(module = "gw121", frozen)]
struct Packet {
    packet: gw121::Packet,
    /// When it arrived, seconds since the Unix epoch (`time.time()`).
    #[pyo3(get)]
    time: f64,
}

#[pymethods]
impl Packet {
    /// The main display; `None` when its mode/range is not decoded (`main_error` says why).
    #[getter]
    fn main(&self) -> Option<Reading> {
        self.packet.main().ok().map(Reading::from)
    }

    /// Why `main` is `None`, else `None`.
    #[getter]
    fn main_error(&self) -> Option<String> {
        self.packet.main().err().map(|e| e.to_string())
    }

    /// The secondary display; `None` in modes and menus where it shows nothing decodable.
    #[getter]
    fn sub(&self) -> Option<Reading> {
        self.packet.sub().ok().map(Reading::from)
    }

    /// The bargraph; `None` when the meter blanks it.
    #[getter]
    fn bar(&self) -> Option<Reading> {
        self.packet.bar().ok().flatten().map(Reading::from)
    }

    #[getter]
    fn flags(&self) -> Flags {
        let f = self.packet.flags();
        Flags {
            auto_range: f.auto_range,
            auto_power_off: f.auto_power_off,
            low_battery: f.low_battery,
            bluetooth: f.bluetooth,
            hold: f.hold,
            relative: f.relative,
            test: f.test,
            memory: f.memory,
            low_pass: f.low_pass,
            peak_1ms: f.peak_1ms,
        }
    }

    /// The 19 bytes as received.
    #[getter]
    fn raw(&self) -> &[u8] {
        self.packet.raw()
    }

    fn __repr__(&self) -> String {
        let show = |r: Option<Reading>| r.map_or("-".to_string(), |r| r.text);
        format!(
            "<Packet main={} sub={} bar={}>",
            show(self.main()),
            show(self.sub()),
            show(self.bar())
        )
    }
}

impl Packet {
    fn new(packet: gw121::Packet) -> Self {
        Packet {
            packet,
            time: now(),
        }
    }
}

/// A BLE device heard by `scan()`.
#[pyclass(module = "gw121", frozen, get_all)]
struct Device {
    address: String,
    name: Option<String>,
    /// Signal strength in dBm; `None` when connected already or not heard (only remembered by BlueZ).
    rssi: Option<i16>,
    connected: bool,
}

#[pymethods]
impl Device {
    fn __repr__(&self) -> String {
        format!(
            "<Device {} {:?} rssi={:?}>",
            self.address,
            self.name.as_deref().unwrap_or("-"),
            self.rssi
        )
    }
}

/// A 121GW connected over BLE. Connects on creation, scanning for the one 121GW nearby without an address;
/// `close()` (or leaving a `with` block) disconnects: a meter left connected stops advertising.
#[pyclass(module = "gw121", frozen)]
struct Meter {
    meter: Mutex<Option<gw121::Meter>>,
    #[pyo3(get)]
    address: String,
}

impl Meter {
    fn with<T>(&self, f: impl FnOnce(&gw121::Meter) -> PyResult<T>) -> PyResult<T> {
        match lock(&self.meter).as_ref() {
            Some(m) => f(m),
            None => Err(Gw121Error::new_err("the meter was closed")),
        }
    }
}

#[pymethods]
impl Meter {
    #[new]
    #[pyo3(signature = (address=None, timeout=10.0))]
    fn new(py: Python<'_>, address: Option<&str>, timeout: f64) -> PyResult<Self> {
        let address = self::address(address)?;
        let timeout = seconds(timeout, "timeout")?;
        let meter = wait(py, gw121::Meter::open(address, timeout))?.map_err(err)?;
        Ok(Meter {
            address: meter.address().to_string(),
            meter: Mutex::new(Some(meter)),
        })
    }

    /// The next packet, waiting at most `timeout` seconds.
    #[pyo3(signature = (timeout=10.0))]
    fn read(&self, py: Python<'_>, timeout: f64) -> PyResult<Packet> {
        let timeout = seconds(timeout, "timeout")?;
        self.with(|m| wait(py, m.read(timeout))?.map_err(err).map(Packet::new))
    }

    /// Packets as they arrive (about two a second), for `duration` seconds or until the loop ends; at most
    /// one per `every` seconds. Raises `NoPacketError` after `timeout` seconds without one.
    #[pyo3(signature = (duration=None, every=None, timeout=10.0))]
    fn log(
        &self,
        py: Python<'_>,
        duration: Option<f64>,
        every: Option<f64>,
        timeout: f64,
    ) -> PyResult<Log> {
        let packets = self.with(|m| wait(py, m.packets())?.map_err(err))?;
        Log::new(packets, None, duration, every, timeout)
    }

    /// Disconnects; safe to call more than once.
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let meter = lock(&self.meter).take();
        if let Some(m) = meter {
            wait(py, m.disconnect())?.map_err(err)?;
        }
        Ok(())
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (*_args))]
    fn __exit__(&self, py: Python<'_>, _args: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }

    fn __repr__(&self) -> String {
        let state = if lock(&self.meter).is_some() {
            "connected"
        } else {
            "closed"
        };
        format!("<Meter {} {state}>", self.address)
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        // gw121::Meter disconnects on drop inside a runtime.
        let _guard = runtime().enter();
        drop(lock(&self.meter).take());
    }
}

struct LogState {
    packets: Option<gw121::Packets>,
    /// The meter `gw121.log()` connected to by itself, disconnected when the log ends.
    owned: Option<gw121::Meter>,
    end: Option<Instant>,
    every: Option<Duration>,
    timeout: Duration,
    last: Option<Instant>,
}

/// An iterator of `Packet`s, from `gw121.log()` or `Meter.log()`. Also a context manager: leaving the `with`
/// block (or `close()`) ends it, and disconnects when `gw121.log()` connected for it.
#[pyclass(module = "gw121", frozen)]
struct Log {
    state: Mutex<LogState>,
}

impl Log {
    fn new(
        packets: gw121::Packets,
        owned: Option<gw121::Meter>,
        duration: Option<f64>,
        every: Option<f64>,
        timeout: f64,
    ) -> PyResult<Self> {
        let duration = duration.map(|d| seconds(d, "duration")).transpose()?;
        Ok(Log {
            state: Mutex::new(LogState {
                packets: Some(packets),
                owned,
                end: duration.map(|d| Instant::now() + d),
                every: every.map(|e| seconds(e, "every")).transpose()?,
                timeout: seconds(timeout, "timeout")?,
                last: None,
            }),
        })
    }
}

impl LogState {
    fn finish(&mut self, py: Python<'_>) -> PyResult<()> {
        self.packets = None;
        if let Some(m) = self.owned.take() {
            wait(py, m.disconnect())?.map_err(err)?;
        }
        Ok(())
    }
}

#[pymethods]
impl Log {
    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Packet>> {
        let mut state = lock(&self.state);
        loop {
            let left = state
                .end
                .map(|e| e.saturating_duration_since(Instant::now()));
            if left.is_some_and(|l| l.is_zero()) {
                state.finish(py)?;
                return Ok(None);
            }
            let timeout = state.timeout;
            let Some(packets) = state.packets.as_mut() else {
                return Ok(None);
            };
            let budget = left.map_or(timeout, |l| l.min(timeout));
            let next = wait(py, async {
                tokio::time::timeout(budget, packets.next_packet()).await
            })?;
            let address = state.owned.as_ref().map(gw121::Meter::address);
            let packet = match next {
                Ok(Ok(p)) => p,
                Ok(Err(e)) => {
                    state.finish(py)?;
                    return Err(err(e));
                }
                Err(_) if left.is_some_and(|l| l <= timeout) => continue, // --duration is up
                Err(_) => {
                    state.finish(py)?;
                    let msg = match address {
                        Some(a) => format!("no valid packet from {a} in {timeout:?}"),
                        None => format!("no valid packet in {timeout:?}"),
                    };
                    return Err(NoPacketError::new_err(msg));
                }
            };
            let now = Instant::now();
            if let Some(every) = state.every
                && state.last.is_some_and(|t| now.duration_since(t) < every)
            {
                continue;
            }
            state.last = Some(now);
            return Ok(Some(Packet::new(packet)));
        }
    }

    /// Stops the log, disconnecting when `gw121.log()` connected for it; safe to call more than once.
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        lock(&self.state).finish(py)
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    #[pyo3(signature = (*_args))]
    fn __exit__(&self, py: Python<'_>, _args: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }
}

impl Drop for Log {
    fn drop(&mut self) {
        let _guard = runtime().enter();
        let mut state = lock(&self.state);
        state.packets = None;
        drop(state.owned.take());
    }
}

/// BLE devices heard within `timeout` seconds: 121GW meters only, or every device with `all=True`.
#[pyfunction]
#[pyo3(signature = (timeout=5.0, all=false))]
fn scan(py: Python<'_>, timeout: f64, all: bool) -> PyResult<Vec<Device>> {
    let timeout = seconds(timeout, "timeout")?;
    let filter = if all { "" } else { gw121::ble::METER_NAME };
    let found = wait(py, async {
        let adapter = gw121::ble::adapter().await?;
        gw121::ble::scan(&adapter, timeout, filter, false).await
    })?
    .map_err(err)?;
    Ok(found
        .into_iter()
        .map(|f| Device {
            address: f.address.to_string(),
            name: f.name,
            rssi: f.rssi,
            connected: f.connected,
        })
        .collect())
}

/// Connects (scanning for the one 121GW nearby without an address), reads one packet and disconnects.
/// `timeout` (seconds) bounds the scan, the connect and the wait for the packet each.
#[pyfunction]
#[pyo3(signature = (address=None, timeout=10.0))]
fn read(py: Python<'_>, address: Option<&str>, timeout: f64) -> PyResult<Packet> {
    let address = self::address(address)?;
    let timeout = seconds(timeout, "timeout")?;
    wait(py, gw121::read(address, timeout))?
        .map_err(err)
        .map(Packet::new)
}

/// Connects (scanning for the one 121GW nearby without an address) and yields packets as they arrive, for
/// `duration` seconds or until the loop ends, at most one per `every` seconds; disconnects when done.
/// `timeout` (seconds) bounds the scan, the connect and each wait for a packet.
#[pyfunction]
#[pyo3(signature = (address=None, duration=None, every=None, timeout=10.0))]
fn log(
    py: Python<'_>,
    address: Option<&str>,
    duration: Option<f64>,
    every: Option<f64>,
    timeout: f64,
) -> PyResult<Log> {
    let address = self::address(address)?;
    let connect_timeout = seconds(timeout, "timeout")?;
    let (meter, packets) = wait(py, async {
        let meter = gw121::Meter::open(address, connect_timeout).await?;
        let packets = meter.packets().await?;
        Ok::<_, gw121::Error>((meter, packets))
    })?
    .map_err(err)?;
    Log::new(packets, Some(meter), duration, every, timeout)
}

/// Decodes 19 bytes of a 121GW packet (from a capture, say) without any Bluetooth.
#[pyfunction]
fn parse(data: &[u8]) -> PyResult<Packet> {
    gw121::Packet::from_slice(data)
        .map(Packet::new)
        .map_err(|e| err(e.into()))
}

#[pymodule]
#[pyo3(name = "gw121")]
fn gw121_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(scan, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    m.add_function(wrap_pyfunction!(log, m)?)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_class::<Meter>()?;
    m.add_class::<Log>()?;
    m.add_class::<Packet>()?;
    m.add_class::<Reading>()?;
    m.add_class::<Flags>()?;
    m.add_class::<Device>()?;
    let py = m.py();
    for (name, exc) in [
        ("Gw121Error", py.get_type::<Gw121Error>()),
        ("BluetoothError", py.get_type::<BluetoothError>()),
        ("NotFoundError", py.get_type::<NotFoundError>()),
        ("AmbiguousError", py.get_type::<AmbiguousError>()),
        ("ConnectError", py.get_type::<ConnectError>()),
        ("NoPacketError", py.get_type::<NoPacketError>()),
        ("DisconnectedError", py.get_type::<DisconnectedError>()),
        ("PacketError", py.get_type::<PacketError>()),
        ("AddressError", py.get_type::<AddressError>()),
    ] {
        m.add(name, exc)?;
    }
    Ok(())
}
