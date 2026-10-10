"""Read an EEVblog 121GW multimeter over Bluetooth LE (Linux, BlueZ).

Mode and range live on the meter's knob and are only ever read, never set. Put the meter in BLE mode first:
hold its 1ms PEAK button until the BT indicator lights.

    import gw121
    print(gw121.read().main)                     # -0.0021 V
    for p in gw121.log(duration=10, every=1):
        print(p.time, p.main.value, p.main.unit)
    with gw121.Meter("88:6B:0F:73:00:01") as m:
        m.read().sub

Calls block with the GIL released; Ctrl+C interrupts them.
"""

from types import TracebackType
from typing import Iterator, Optional

__version__: str

class Gw121Error(Exception):
    """Base class of every gw121 error."""

class BluetoothError(Gw121Error):
    """BlueZ failed: no adapter, Bluetooth off or blocked, permission denied, ..."""

class NotFoundError(Gw121Error):
    """A scan heard no 121GW: is it on, in BLE mode (hold 1ms PEAK until BT lights) and in range?"""

class AmbiguousError(Gw121Error):
    """A scan heard more than one 121GW: pass the address of one."""

class ConnectError(Gw121Error):
    """Could not connect: the address was not heard, connecting timed out, or it is not a 121GW."""

class NoPacketError(Gw121Error):
    """Connected, but no valid packet arrived within the timeout."""

class DisconnectedError(Gw121Error):
    """The meter went away: switched off, out of range, or asleep."""

class PacketError(Gw121Error):
    """Bytes that are not a valid 121GW packet."""

class AddressError(Gw121Error):
    """Text that is not a BLE address."""

class Reading:
    """One display's value: main, sub or the bargraph. `str()` gives it as the display shows it: `-0.0021 V`."""

    @property
    def value(self) -> float:
        """The value in `unit`, rounded to the digits the display shows; `inf`/`-inf` on overload."""
    @property
    def digits(self) -> int:
        """Decimal places the display shows; negative for a range shown in tens, hundreds, ..."""
    @property
    def quantity(self) -> str:
        """`voltage`, `current`, `power`, `resistance`, `capacitance`, `temperature`, `frequency`, `period`,
        `dutycycle`, `continuity`, `gain`, or `count` (the bargraph)."""
    @property
    def unit(self) -> str:
        """`V`, `A`, `VA`, `dBm`, `ohm`, `F`, `degC`, `degF`, `Hz`, `s`, `%`, or `` (the bargraph)."""
    @property
    def overload(self) -> bool: ...
    @property
    def ac(self) -> bool: ...
    @property
    def dc(self) -> bool: ...
    def __float__(self) -> float: ...

class Flags:
    """The meter's own flags, not tied to one display."""

    @property
    def auto_range(self) -> bool: ...
    @property
    def auto_power_off(self) -> bool: ...
    @property
    def low_battery(self) -> bool: ...
    @property
    def bluetooth(self) -> bool: ...
    @property
    def hold(self) -> bool: ...
    @property
    def relative(self) -> bool: ...
    @property
    def test(self) -> bool: ...
    @property
    def memory(self) -> bool: ...
    @property
    def low_pass(self) -> bool:
        """1 kHz low-pass filter engaged."""
    @property
    def peak_1ms(self) -> bool:
        """1 ms peak capture active."""

class Packet:
    """One packet from the meter: both displays, the bargraph and the flags at one moment."""

    @property
    def time(self) -> float:
        """When it arrived, seconds since the Unix epoch (like `time.time()`)."""
    @property
    def main(self) -> Optional[Reading]:
        """The main display; `None` when its mode/range is not decoded (`main_error` says why)."""
    @property
    def main_error(self) -> Optional[str]:
        """Why `main` is `None`, else `None`."""
    @property
    def sub(self) -> Optional[Reading]:
        """The secondary display; `None` in modes and menus where it shows nothing decodable."""
    @property
    def bar(self) -> Optional[Reading]:
        """The bargraph; `None` when the meter blanks it."""
    @property
    def flags(self) -> Flags: ...
    @property
    def raw(self) -> bytes:
        """The 19 bytes as received."""

class Device:
    """A BLE device heard by `scan()`."""

    @property
    def address(self) -> str: ...
    @property
    def name(self) -> Optional[str]: ...
    @property
    def rssi(self) -> Optional[int]:
        """Signal strength in dBm; `None` when connected already or not heard (only remembered by BlueZ)."""
    @property
    def connected(self) -> bool: ...

class Log:
    """An iterator of `Packet`s from `log()` or `Meter.log()`. Leaving a `with` block (or `close()`) ends it,
    and disconnects when `log()` connected for it."""

    def __iter__(self) -> Iterator[Packet]: ...
    def __next__(self) -> Packet: ...
    def close(self) -> None: ...
    def __enter__(self) -> "Log": ...
    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc: Optional[BaseException],
        tb: Optional[TracebackType],
    ) -> bool: ...

class Meter:
    """A 121GW connected over BLE. Connects on creation, scanning for the one 121GW nearby without an
    address; `close()` (or leaving a `with` block) disconnects: a meter left connected stops advertising."""

    @property
    def address(self) -> str: ...
    def __init__(self, address: Optional[str] = None, timeout: float = 10.0) -> None: ...
    def read(self, timeout: float = 10.0) -> Packet:
        """The next packet, waiting at most `timeout` seconds."""
    def log(
        self, duration: Optional[float] = None, every: Optional[float] = None, timeout: float = 10.0
    ) -> Log:
        """Packets as they arrive (about two a second), for `duration` seconds or until the loop ends; at most
        one per `every` seconds. Raises `NoPacketError` after `timeout` seconds without one."""
    def close(self) -> None:
        """Disconnects; safe to call more than once."""
    def __enter__(self) -> "Meter": ...
    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc: Optional[BaseException],
        tb: Optional[TracebackType],
    ) -> bool: ...

def scan(timeout: float = 5.0, all: bool = False) -> list[Device]:
    """BLE devices heard within `timeout` seconds: 121GW meters only, or every device with `all=True`."""

def read(address: Optional[str] = None, timeout: float = 10.0) -> Packet:
    """Connects (scanning for the one 121GW nearby without an address), reads one packet and disconnects.
    `timeout` (seconds) bounds the scan, the connect and the wait for the packet each."""

def log(
    address: Optional[str] = None,
    duration: Optional[float] = None,
    every: Optional[float] = None,
    timeout: float = 10.0,
) -> Log:
    """Connects (scanning for the one 121GW nearby without an address) and yields packets as they arrive, for
    `duration` seconds or until the loop ends, at most one per `every` seconds; disconnects when done.
    `timeout` (seconds) bounds the scan, the connect and each wait for a packet."""

def parse(data: bytes) -> Packet:
    """Decodes 19 bytes of a 121GW packet (from a capture, say) without any Bluetooth."""
