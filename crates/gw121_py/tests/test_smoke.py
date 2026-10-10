"""Offline smoke test: the module imports, decodes the sigrok wiki's recorded packets, and maps errors.
No Bluetooth adapter or meter needed."""

import math

import pytest

import gw121

# From the sigrok wiki's BLE capture (https://sigrok.org/wiki/EEVBlog_121GW), as in the Rust unit tests.
PKT1 = bytes.fromhex("f2178001210140001564 0100f000000e4000ca".replace(" ", ""))
PKT3 = bytes.fromhex("f217800121010000 1f6401 00ef00000e40009f".replace(" ", ""))


def test_version():
    assert gw121.__version__.count(".") == 2


def test_parse_recorded_packet():
    p = gw121.parse(PKT1)
    assert p.main is not None and p.main_error is None
    assert p.main.quantity == "voltage"
    assert p.main.unit == "V"
    assert p.main.dc and not p.main.ac
    assert math.isclose(p.main.value, -0.0021)
    assert float(p.main) == p.main.value
    assert str(p.main) == "-0.0021 V"
    assert str(p.sub) == "24.0 degC"
    assert p.bar is not None and p.bar.value == 0.0
    assert p.flags.auto_power_off and not p.flags.hold
    assert p.raw == PKT1
    assert p.time > 0


def test_values_are_rounded_to_display_digits():
    p = gw121.parse(PKT3)
    assert p.main.value == 0.0031
    assert p.sub.value == 23.9


def test_errors_map_to_python_exceptions():
    with pytest.raises(gw121.PacketError, match="19 bytes"):
        gw121.parse(PKT1[:18])
    bad = bytearray(PKT1)
    bad[18] ^= 0xFF
    with pytest.raises(gw121.Gw121Error, match="checksum"):
        gw121.parse(bytes(bad))
    with pytest.raises(gw121.AddressError):
        gw121.read(address="not an address")
    with pytest.raises(ValueError):
        gw121.read(timeout=-1)
    for exc in (gw121.NotFoundError, gw121.DisconnectedError, gw121.NoPacketError, gw121.ConnectError):
        assert issubclass(exc, gw121.Gw121Error)
