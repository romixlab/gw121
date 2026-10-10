//! The 121GW's 19-byte binary packet (one `main` display, one `sub` display, one bargraph, global flags).
//!
//! Ported as facts (bit positions, mode numbers, mode/range scale tables), not copied code, from libsigrok's
//! GPL-2.0-or-later `eev121gw` DMM parser (`src/dmm/eev121gw.c`, Gerhard Sittig, 2018), the only published,
//! byte-accurate decoder for this format; see AGENTS.md for the provenance chain (sigrok wiki -> libsigrok
//! source). The vendor's own "Revised-Packet-Format-Blob-V2" PDF (linked from the product page) was not
//! reachable from here, so libsigrok's reverse-engineered layout is the source of truth.
//!
//! Byte layout (offsets 0-18):
//!
//! ```text
//! 0      START_BYTE (0xF2)
//! 1..4   serial: year(8) month(4) number(20), packed big-endian, informational only
//! 5      main mode byte: val_upper(2) rsv(1) mode(5)
//! 6      main range byte: ofl(1) sign(1) degC(1) degF(1) range(4)
//! 7..8   main value, big-endian u16 (low 16 bits of an 18-bit mantissa)
//! 9      sub mode byte: mode(8)
//! 10     sub range byte: ofl(1) sign(1) k(1) hz(1) rsv(1) point(3)
//! 11..12 sub value, big-endian u16
//! 13     bar status byte: rsv(3) use(1) rsv150(1) sign(1) range(2)
//! 14     bar value byte: rsv(2) rsv5(1) value(5)
//! 15     icon status 1: degC(1) 1khz(1) 1ms_peak(1) dc_ac(2) auto(1) apo(1) bat(1)
//! 16     icon status 2: degF(1) bt(1) unknown(1) rel(1) dbm(1) minmax(3)
//! 17     icon status 3: rsv(1) test(1) mem(2) ahold(2) ac(1) dc(1)
//! 18     checksum: XOR of bytes 0..17
//! ```

/// Packet length in bytes.
pub const PACKET_SIZE: usize = 19;
/// First byte of every packet.
pub const START_BYTE: u8 = 0xF2;

const OFF_MAIN_MODE: usize = 5;
const OFF_MAIN_RANGE: usize = 6;
const OFF_MAIN_VAL_H: usize = 7;
const OFF_SUB_MODE: usize = 9;
const OFF_SUB_RANGE: usize = 10;
const OFF_SUB_VAL_H: usize = 11;
const OFF_BAR_STATUS: usize = 13;
const OFF_BAR_VALUE: usize = 14;
const OFF_ICON_1: usize = 15;
const OFF_ICON_2: usize = 16;
const OFF_ICON_3: usize = 17;
const OFF_CHECKSUM: usize = 18;

/// Extracts `len` bits of `byte` starting at bit `pos` (0 = least significant).
const fn bits(byte: u8, pos: u32, len: u32) -> u8 {
    (byte >> pos) & ((1u16 << len) - 1) as u8
}

const fn bit(byte: u8, pos: u32) -> bool {
    bits(byte, pos, 1) != 0
}

/// A packet failed validation, or a field holds a value no known mode/range table covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    #[error("a packet is {PACKET_SIZE} bytes, got {0}")]
    WrongLength(usize),
    #[error("first byte is {0:#04x}, expected {START_BYTE:#04x}")]
    BadStartByte(u8),
    #[error("a reserved bit is set (byte {0}: {1:#010b})")]
    ReservedBitSet(usize, u8),
    #[error("checksum mismatch: packet says {0:#04x}, computed {1:#04x}")]
    BadChecksum(u8, u8),
    #[error("main mode {0} has no known range table")]
    UnknownMainMode(u8),
    #[error("sub mode {0} has no known range table")]
    UnknownSubMode(u8),
    #[error("main mode {mode} range {range} is not in the mode's range table")]
    UnknownMainRange { mode: u8, range: u8 },
    #[error("sub mode {mode} decimal point {point} is not in the mode's range table")]
    UnknownSubRange { mode: u8, point: u8 },
    #[error("bargraph range {0} is not one of the four known ranges")]
    UnknownBarRange(u8),
}

/// Validates the start byte, the bits the firmware never sets, and the XOR checksum. Does not interpret any
/// other field.
pub fn validate(buf: &[u8; PACKET_SIZE]) -> Result<(), ParseError> {
    if buf[0] != START_BYTE {
        return Err(ParseError::BadStartByte(buf[0]));
    }
    // Reserved bits the real device was never observed to set (libsigrok eev121gw.c
    // sr_eev121gw_packet_valid). Bit 5 of the bar value byte was seen set in FREQ/OHM on a real meter and is
    // deliberately not checked here, matching upstream's disabled check.
    if bit(buf[OFF_MAIN_MODE], 5) {
        return Err(ParseError::ReservedBitSet(
            OFF_MAIN_MODE,
            buf[OFF_MAIN_MODE],
        ));
    }
    if bit(buf[OFF_SUB_RANGE], 3) {
        return Err(ParseError::ReservedBitSet(
            OFF_SUB_RANGE,
            buf[OFF_SUB_RANGE],
        ));
    }
    if bits(buf[OFF_BAR_STATUS], 5, 3) != 0 {
        return Err(ParseError::ReservedBitSet(
            OFF_BAR_STATUS,
            buf[OFF_BAR_STATUS],
        ));
    }
    if bits(buf[OFF_BAR_VALUE], 6, 2) != 0 {
        return Err(ParseError::ReservedBitSet(
            OFF_BAR_VALUE,
            buf[OFF_BAR_VALUE],
        ));
    }
    if bit(buf[OFF_ICON_3], 7) {
        return Err(ParseError::ReservedBitSet(OFF_ICON_3, buf[OFF_ICON_3]));
    }
    let want = buf[OFF_CHECKSUM];
    let got = buf[..OFF_CHECKSUM].iter().fold(0u8, |acc, b| acc ^ b);
    if got != want {
        return Err(ParseError::BadChecksum(want, got));
    }
    Ok(())
}

/// What a reading measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    Voltage,
    Current,
    Power,
    Resistance,
    Capacitance,
    Temperature,
    Frequency,
    Period,
    DutyCycle,
    Continuity,
    Gain,
    /// The bargraph: a dimensionless count of lit segments, scaled like its display's main reading.
    Count,
}

impl Quantity {
    /// Lowercase name, as in JSON output (`voltage`, `dutycycle`).
    pub const fn name(self) -> &'static str {
        match self {
            Quantity::Voltage => "voltage",
            Quantity::Current => "current",
            Quantity::Power => "power",
            Quantity::Resistance => "resistance",
            Quantity::Capacitance => "capacitance",
            Quantity::Temperature => "temperature",
            Quantity::Frequency => "frequency",
            Quantity::Period => "period",
            Quantity::DutyCycle => "dutycycle",
            Quantity::Continuity => "continuity",
            Quantity::Gain => "gain",
            Quantity::Count => "count",
        }
    }
}

/// The unit a reading's value is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Volt,
    Ampere,
    VoltAmpere,
    DecibelVolt,
    Ohm,
    Farad,
    Celsius,
    Fahrenheit,
    Hertz,
    Second,
    Percent,
    Unitless,
}

impl Unit {
    /// Short symbol, as printed in text output.
    pub const fn symbol(self) -> &'static str {
        match self {
            Unit::Volt => "V",
            Unit::Ampere => "A",
            Unit::VoltAmpere => "VA",
            Unit::DecibelVolt => "dBm",
            Unit::Ohm => "ohm",
            Unit::Farad => "F",
            Unit::Celsius => "degC",
            Unit::Fahrenheit => "degF",
            Unit::Hertz => "Hz",
            Unit::Second => "s",
            Unit::Percent => "%",
            Unit::Unitless => "",
        }
    }
}

/// One display's value: already scaled to its unit, signed, `f64::INFINITY` on overload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub value: f64,
    /// Significant digits the meter's display shows (can be negative: e.g. -3 for a resistance shown in whole
    /// kilohms of a megohm range). Matches libsigrok's `analog.encoding.digits`.
    pub digits: i32,
    pub quantity: Quantity,
    pub unit: Unit,
    pub overload: bool,
    pub ac: bool,
    pub dc: bool,
}

impl Reading {
    /// The value with the meter's decimals only (3.2971, not 3.2971000000000004); `digits` may be negative
    /// (whole kilohms of a megohm range). Infinite on overload, as `value`.
    pub fn rounded(&self) -> f64 {
        if !self.value.is_finite() {
            return self.value;
        }
        let scale = 10f64.powi(self.digits);
        (self.value * scale).round() / scale
    }
}

/// The value at the precision the display shows, `OVER`/`-OVER` on overload, and the unit symbol:
/// `-0.0021 V`, `24.0 degC`.
impl std::fmt::Display for Reading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.value.is_infinite() {
            let sign = if self.value.is_sign_negative() {
                "-"
            } else {
                ""
            };
            write!(f, "{sign}OVER")?;
        } else if self.digits >= 0 {
            write!(f, "{:.*}", self.digits as usize, self.value)?;
        } else {
            let scale = 10f64.powi(-self.digits);
            write!(f, "{:.0}", (self.value / scale).round() * scale)?;
        }
        match self.unit.symbol() {
            "" => Ok(()),
            unit => write!(f, " {unit}"),
        }
    }
}

/// Global device state carried in the packet's "icon" bytes: not tied to one display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    pub auto_range: bool,
    pub auto_power_off: bool,
    pub low_battery: bool,
    pub bluetooth: bool,
    pub hold: bool,
    pub relative: bool,
    pub test: bool,
    /// Memory/reference indicator; libsigrok leaves its exact meaning unconfirmed (2-bit field, any nonzero).
    pub memory: bool,
    /// 1 kHz low-pass filter engaged (main display).
    pub low_pass: bool,
    /// 1 ms peak capture mode active.
    pub peak_1ms: bool,
    /// Unconfirmed upstream (icon status 2, bit 5); possibly a 20 mA current-loop indicator.
    pub unknown_bit: bool,
}

fn flags(buf: &[u8; PACKET_SIZE]) -> Flags {
    let s1 = buf[OFF_ICON_1];
    let s2 = buf[OFF_ICON_2];
    let s3 = buf[OFF_ICON_3];
    Flags {
        auto_range: bit(s1, 2),
        auto_power_off: bit(s1, 1),
        low_battery: bit(s1, 0),
        low_pass: bit(s1, 6),
        peak_1ms: bit(s1, 5),
        bluetooth: bit(s2, 6),
        unknown_bit: bit(s2, 5),
        relative: bit(s2, 4),
        test: bit(s3, 6),
        memory: bits(s3, 4, 2) != 0,
        hold: bits(s3, 2, 2) != 0,
    }
}

/// `main` and `sub` display modes shared with the main display's 0..=24 range (ported from
/// `enum mode_codes` in libsigrok's eev121gw.c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainMode {
    LowZ,
    DcV,
    AcV,
    DcMv,
    AcMv,
    Temp,
    Freq,
    Period,
    Duty,
    Res,
    Cont,
    Diode,
    Cap,
    AcUva,
    AcMva,
    AcVa,
    AcUa,
    DcUa,
    AcMa,
    DcMa,
    AcA,
    DcA,
    DcUva,
    DcMva,
    DcVa,
}

impl TryFrom<u8> for MainMode {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, ()> {
        use MainMode::*;
        Ok(match v {
            0 => LowZ,
            1 => DcV,
            2 => AcV,
            3 => DcMv,
            4 => AcMv,
            5 => Temp,
            6 => Freq,
            7 => Period,
            8 => Duty,
            9 => Res,
            10 => Cont,
            11 => Diode,
            12 => Cap,
            13 => AcUva,
            14 => AcMva,
            15 => AcVa,
            16 => AcUa,
            17 => DcUa,
            18 => AcMa,
            19 => DcMa,
            20 => AcA,
            21 => DcA,
            22 => DcUva,
            23 => DcMva,
            24 => DcVa,
            _ => return Err(()),
        })
    }
}

/// Decimal places for the main display's mantissa, by mode and range index. `factor == digits` for every
/// entry in libsigrok's tables, so one number does both jobs here.
fn main_decimals(mode: MainMode, range: u8) -> Option<i32> {
    use MainMode::*;
    let table: &[i32] = match mode {
        LowZ => &[1],
        DcV | AcV => &[4, 3, 2, 1],
        DcMv | AcMv => &[6, 5],
        Temp => &[1],
        Freq => &[3, 2, 1, 0, -1],
        Period => &[7, 6, 5],
        Duty => &[1],
        Res => &[3, 2, 1, 0, -1, -2, -3],
        Cont => &[2],
        Diode => &[4, 3],
        Cap => &[11, 10, 9, 8, 7, 5],
        AcVa | DcVa => &[4, 3, 3, 2],
        AcMva | DcMva => &[6, 5, 5, 4],
        AcUva | DcUva => &[8, 7, 7, 6],
        AcA | DcA => &[5, 4, 3],
        AcMa | DcMa => &[7, 6],
        AcUa | DcUa => &[9, 8],
    };
    table.get(range as usize).copied()
}

/// `sub` display modes that don't overlap the main 0..=24 set (ported from the 100+ entries of
/// libsigrok's `enum mode_codes`). Modes the vendor app shows only during setup (APO, LCD, date/time) are not
/// represented: the upstream parser refuses them too (`SR_ERR_NA`), and the CLI treats an unknown sub mode the
/// same way (no sub reading that packet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubMode {
    Main(MainMode),
    TempC,
    TempF,
    Batt,
    ContParam(u8),
    Dbm,
}

impl TryFrom<u8> for SubMode {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, ()> {
        Ok(match v {
            0..=24 => SubMode::Main(MainMode::try_from(v)?),
            100 => SubMode::TempC,
            105 => SubMode::TempF,
            110 => SubMode::Batt,
            170..=173 => SubMode::ContParam(v - 170),
            180 => SubMode::Dbm,
            _ => return Err(()),
        })
    }
}

/// Decimal places for the sub display's mantissa; `point` is the sub range byte's 3-bit field, used directly as
/// an index (the sub display encodes the decimal point position per mode, not a range index like main).
fn sub_decimals(mode: SubMode, point: u8) -> Option<i32> {
    use MainMode::*;
    let table: &[Option<i32>] = match mode {
        SubMode::Main(DcV) | SubMode::Main(AcV) => &[None, None, None, Some(3), Some(4)],
        SubMode::Main(DcA) | SubMode::Main(AcA) | SubMode::Main(DcMa) | SubMode::Main(AcMa) => {
            &[None, None, Some(5), Some(6), Some(7)]
        }
        SubMode::Main(DcUa) | SubMode::Main(AcUa) => &[None, None, None, None, Some(7)],
        SubMode::Main(Freq) => &[None, Some(1), Some(2), Some(3)],
        SubMode::Main(Diode) => &[Some(0)],
        SubMode::TempC | SubMode::TempF => &[None, Some(1)],
        SubMode::Batt => &[None, Some(1)],
        SubMode::Dbm => &[None, Some(1), Some(2), Some(3)],
        SubMode::ContParam(_) => &[Some(0)],
        _ => return None,
    };
    table.get(point as usize).copied().flatten()
}

fn mantissa_to_value(raw: u32, decimals: i32) -> f64 {
    raw as f64 * 10f64.powi(-decimals)
}

/// One parsed 121GW packet.
#[derive(Debug, Clone, Copy)]
pub struct Packet {
    raw: [u8; PACKET_SIZE],
}

impl Packet {
    /// Validates and wraps a 19-byte buffer. See [`validate`] for what is checked.
    pub fn parse(buf: [u8; PACKET_SIZE]) -> Result<Self, ParseError> {
        validate(&buf)?;
        Ok(Packet { raw: buf })
    }

    /// Like [`Packet::parse`], from a slice that must be exactly [`PACKET_SIZE`] bytes.
    pub fn from_slice(buf: &[u8]) -> Result<Self, ParseError> {
        let buf: [u8; PACKET_SIZE] = buf
            .try_into()
            .map_err(|_| ParseError::WrongLength(buf.len()))?;
        Self::parse(buf)
    }

    /// The 19 bytes as received.
    pub fn raw(&self) -> &[u8; PACKET_SIZE] {
        &self.raw
    }

    pub fn flags(&self) -> Flags {
        flags(&self.raw)
    }

    /// The main display: the large seven-segment readout.
    pub fn main(&self) -> Result<Reading, ParseError> {
        let raw_mode = self.raw[OFF_MAIN_MODE];
        let raw_range = self.raw[OFF_MAIN_RANGE];
        let mode_num = bits(raw_mode, 0, 5);
        let mode =
            MainMode::try_from(mode_num).map_err(|_| ParseError::UnknownMainMode(mode_num))?;
        let range = bits(raw_range, 0, 4);
        let decimals = main_decimals(mode, range).ok_or(ParseError::UnknownMainRange {
            mode: mode_num,
            range,
        })?;

        let val_h = u16::from_be_bytes([self.raw[OFF_MAIN_VAL_H], self.raw[OFF_MAIN_VAL_H + 1]]);
        let upper = bits(raw_mode, 6, 2) as u32;
        let mantissa = (val_h as u32) | (upper << 16);
        let overload = bit(raw_range, 7);

        let icon3 = self.raw[OFF_ICON_3];
        let icon1 = self.raw[OFF_ICON_1];
        use MainMode::*;
        let (quantity, unit, mut ac, mut dc) = match mode {
            LowZ => (Quantity::Voltage, Unit::Volt, false, false),
            DcV | DcMv => (Quantity::Voltage, Unit::Volt, false, true),
            AcV | AcMv => (Quantity::Voltage, Unit::Volt, true, false),
            Temp => (
                Quantity::Temperature,
                // DEGC (bit 5) and DEGF (bit 4) are independent bits upstream; the firmware was only ever
                // observed to set exactly one, so DEGF absent is treated as Celsius.
                if bit(raw_range, 4) {
                    Unit::Fahrenheit
                } else {
                    Unit::Celsius
                },
                false,
                false,
            ),
            Freq => (Quantity::Frequency, Unit::Hertz, false, false),
            Period => (Quantity::Period, Unit::Second, false, false),
            Duty => (Quantity::DutyCycle, Unit::Percent, false, false),
            Res => (Quantity::Resistance, Unit::Ohm, false, false),
            Cont => (Quantity::Continuity, Unit::Ohm, false, false),
            Diode => (Quantity::Voltage, Unit::Volt, false, true),
            Cap => (Quantity::Capacitance, Unit::Farad, false, false),
            AcUva | AcMva | AcVa => (Quantity::Power, Unit::VoltAmpere, true, false),
            DcUva | DcMva | DcVa => (Quantity::Power, Unit::VoltAmpere, false, true),
            AcUa | AcMa | AcA => (Quantity::Current, Unit::Ampere, true, false),
            DcUa | DcMa | DcA => (Quantity::Current, Unit::Ampere, false, true),
        };
        // The global AC/DC icon and status-3 AC/DC bits can add to (never remove) what the mode already
        // implies; LowZ takes its reported AC/DC entirely from these (it has none of its own).
        match bits(icon1, 3, 2) {
            1 => dc = true,
            2 => ac = true,
            3 => {
                ac = true;
                dc = true;
            }
            _ => {}
        }
        if bit(icon3, 1) {
            ac = true;
        }
        if bit(icon3, 0) {
            dc = true;
        }

        // LowZ's sign is only meaningful in DC (a separate question from the AC/DC icons reported above:
        // upstream computes this from the raw status-3 bits directly, before the icons are folded in).
        let low_z_is_dc = bit(icon3, 0) && !bit(icon3, 1);
        let use_sign = matches!(
            mode,
            DcV | DcMv | Temp | DcUva | DcMva | DcVa | DcUa | DcMa | DcA
        ) || (mode == LowZ && low_z_is_dc);
        let negative = use_sign && bit(raw_range, 6);

        let mut value = mantissa_to_value(mantissa, decimals);
        if overload {
            value = f64::INFINITY;
        }
        if negative {
            value = -value;
        }
        Ok(Reading {
            value,
            digits: decimals,
            quantity,
            unit,
            overload,
            ac,
            dc,
        })
    }

    /// The sub display: the smaller secondary readout (often temperature or the diode/battery test voltage).
    pub fn sub(&self) -> Result<Reading, ParseError> {
        let raw_mode = self.raw[OFF_SUB_MODE];
        let raw_range = self.raw[OFF_SUB_RANGE];
        let mode = SubMode::try_from(raw_mode).map_err(|_| ParseError::UnknownSubMode(raw_mode))?;
        let point = bits(raw_range, 0, 3);
        let mut decimals = sub_decimals(mode, point).ok_or(ParseError::UnknownSubRange {
            mode: raw_mode,
            point,
        })?;

        let mantissa =
            u16::from_be_bytes([self.raw[OFF_SUB_VAL_H], self.raw[OFF_SUB_VAL_H + 1]]) as u32;
        let mut overload = bit(raw_range, 7);

        use MainMode::*;
        let (quantity, unit) = match mode {
            SubMode::Main(DcV) | SubMode::Main(AcV) => (Quantity::Voltage, Unit::Volt),
            SubMode::Main(DcA) | SubMode::Main(AcA) | SubMode::Main(DcMa) | SubMode::Main(AcMa) => {
                (Quantity::Current, Unit::Ampere)
            }
            SubMode::Main(DcUa) | SubMode::Main(AcUa) => (Quantity::Current, Unit::Ampere),
            SubMode::Main(Freq) => (Quantity::Frequency, Unit::Hertz),
            SubMode::Main(Diode) => (Quantity::Voltage, Unit::Volt),
            SubMode::TempC => (Quantity::Temperature, Unit::Celsius),
            SubMode::TempF => (Quantity::Temperature, Unit::Fahrenheit),
            SubMode::Batt => (Quantity::Voltage, Unit::Volt),
            SubMode::Dbm => (Quantity::Gain, Unit::DecibelVolt),
            SubMode::ContParam(_) => (Quantity::Resistance, Unit::Ohm),
            SubMode::Main(_) => return Err(ParseError::UnknownSubMode(raw_mode)),
        };

        if mode == SubMode::Main(Freq) {
            // The sub display shows frequency in Hz or kHz (the "k" bit), never overflows.
            if bit(raw_range, 5) {
                decimals -= 3;
            }
            overload = false;
        }

        let use_sign = matches!(
            mode,
            SubMode::Main(DcV) | SubMode::Main(AcV) | SubMode::Main(DcA) | SubMode::Main(AcA)
        ) || matches!(
            mode,
            SubMode::TempC | SubMode::TempF | SubMode::Batt | SubMode::Dbm
        );
        let negative = use_sign && bit(raw_range, 6);

        let value = if let SubMode::ContParam(code) = mode {
            // The continuity/break threshold the user configured (30 or 300 ohm), not a live measurement.
            if code & 0x02 != 0 { 300.0 } else { 30.0 }
        } else {
            let mut v = mantissa_to_value(mantissa, decimals);
            if overload {
                v = f64::INFINITY;
            }
            if negative {
                v = -v;
            }
            v
        };

        Ok(Reading {
            value,
            digits: decimals,
            quantity,
            unit,
            overload,
            ac: false,
            dc: false,
        })
    }

    /// The bargraph: up to 25 lit segments, scaled to the same quantity/unit as the main display's range.
    /// `Ok(None)` when the firmware blanks the bargraph for the current mode.
    pub fn bar(&self) -> Result<Option<Reading>, ParseError> {
        let status = self.raw[OFF_BAR_STATUS];
        if bit(status, 4) {
            // "use" bit set means blanked, not "in use" (negative logic, matches libsigrok).
            return Ok(None);
        }
        let value_byte = self.raw[OFF_BAR_VALUE];
        let segments = bits(value_byte, 0, 5).min(25) as f64;
        let negative = bit(status, 2);
        let range = bits(status, 0, 2);
        let (scaled, digits) = match range {
            0 => (segments * (5000.0 / 25.0), 1), // full scale 5.0, steps of 0.2
            1 => (segments * (50.0 / 25.0), 0),   // full scale 50, steps of 2
            2 => (segments * (500.0 / 25.0), -1), // full scale 500, steps of 20
            3 => (segments * (1000.0 / 25.0), -1), // full scale 1000, steps of 40
            r => return Err(ParseError::UnknownBarRange(r)),
        };
        let decimals = if range == 0 { 3 } else { 0 };
        let mut value = mantissa_to_value(scaled as u32, decimals);
        if negative {
            value = -value;
        }
        Ok(Some(Reading {
            value,
            digits,
            quantity: Quantity::Count,
            unit: Unit::Unitless,
            overload: false,
            ac: false,
            dc: false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// From the sigrok wiki's own BLE capture (<https://sigrok.org/wiki/EEVBlog_121GW>, "Protocol" > "BLE"):
    /// `gatttool -b DEV_ADDR --char-write-req -a 9 -n 0300 --listen`, three consecutive packets reassembled
    /// from the printed indications. Checksums verified by hand before trusting them as fixtures.
    const PKT1: [u8; 19] = [
        0xf2, 0x17, 0x80, 0x01, 0x21, 0x01, 0x40, 0x00, 0x15, 0x64, 0x01, 0x00, 0xf0, 0x00, 0x00,
        0x0e, 0x40, 0x00, 0xca,
    ];
    const PKT2: [u8; 19] = [
        0xf2, 0x17, 0x80, 0x01, 0x21, 0x01, 0x40, 0x00, 0x01, 0x64, 0x01, 0x00, 0xf0, 0x00, 0x00,
        0x0e, 0x40, 0x00, 0xde,
    ];
    const PKT3: [u8; 19] = [
        0xf2, 0x17, 0x80, 0x01, 0x21, 0x01, 0x00, 0x00, 0x1f, 0x64, 0x01, 0x00, 0xef, 0x00, 0x00,
        0x0e, 0x40, 0x00, 0x9f,
    ];

    #[test]
    fn checksums_from_the_wiki_capture_are_valid() {
        validate(&PKT1).unwrap();
        validate(&PKT2).unwrap();
        validate(&PKT3).unwrap();
    }

    #[test]
    fn decodes_dc_volts_main_and_celsius_sub() {
        let p = Packet::parse(PKT1).unwrap();
        let main = p.main().unwrap();
        assert_eq!(main.quantity, Quantity::Voltage);
        assert_eq!(main.unit, Unit::Volt);
        assert!(main.dc && !main.ac);
        assert!((main.value - -0.0021).abs() < 1e-9);

        let sub = p.sub().unwrap();
        assert_eq!(sub.quantity, Quantity::Temperature);
        assert_eq!(sub.unit, Unit::Celsius);
        assert!((sub.value - 24.0).abs() < 1e-9);

        let flags = p.flags();
        assert!(flags.auto_power_off);
        assert!(!flags.low_battery);
        assert!(!flags.hold);
        assert!(!flags.relative);
    }

    #[test]
    fn consecutive_samples_track_small_changes() {
        let v2 = Packet::parse(PKT2).unwrap().main().unwrap().value;
        let v3 = Packet::parse(PKT3).unwrap().main().unwrap().value;
        assert!((v2 - -0.0001).abs() < 1e-9);
        assert!((v3 - 0.0031).abs() < 1e-9);
        let t3 = Packet::parse(PKT3).unwrap().sub().unwrap().value;
        assert!((t3 - 23.9).abs() < 1e-9);
    }

    #[test]
    fn auto_range_bar_is_present_and_scaled_to_5v_range() {
        let bar = Packet::parse(PKT1).unwrap().bar().unwrap().unwrap();
        assert_eq!(bar.value, 0.0);
    }

    fn reading(value: f64, digits: i32, unit: Unit) -> Reading {
        Reading {
            value,
            digits,
            quantity: Quantity::Voltage,
            unit,
            overload: false,
            ac: false,
            dc: true,
        }
    }

    #[test]
    fn formats_readings() {
        assert_eq!(reading(-0.0021, 4, Unit::Volt).to_string(), "-0.0021 V");
        assert_eq!(reading(24.0, 1, Unit::Celsius).to_string(), "24.0 degC");
        assert_eq!(
            reading(50_000_000.0, -3, Unit::Ohm).to_string(),
            "50000000 ohm"
        );
        assert_eq!(reading(f64::INFINITY, 1, Unit::Volt).to_string(), "OVER V");
        assert_eq!(
            reading(f64::NEG_INFINITY, 1, Unit::Volt).to_string(),
            "-OVER V"
        );
    }

    #[test]
    fn rounds_to_meter_digits() {
        assert_eq!(reading(3.2971000000000004, 4, Unit::Volt).rounded(), 3.2971);
        assert_eq!(reading(1234.0, -3, Unit::Ohm).rounded(), 1000.0);
    }

    #[test]
    fn from_slice_checks_the_length() {
        assert_eq!(
            Packet::from_slice(&PKT1[..18]).unwrap_err(),
            ParseError::WrongLength(18)
        );
        assert_eq!(Packet::from_slice(&PKT1).unwrap().raw(), &PKT1);
    }

    #[test]
    fn rejects_bad_start_byte() {
        let mut buf = PKT1;
        buf[0] = 0x00;
        assert_eq!(validate(&buf), Err(ParseError::BadStartByte(0x00)));
    }

    #[test]
    fn rejects_bad_checksum() {
        let mut buf = PKT1;
        buf[18] ^= 0xFF;
        assert!(matches!(validate(&buf), Err(ParseError::BadChecksum(_, _))));
    }

    #[test]
    fn rejects_reserved_bit_set() {
        let mut buf = PKT1;
        buf[OFF_MAIN_MODE] |= 1 << 5;
        buf[18] ^= 1 << 5; // keep the checksum consistent so this test isolates the reserved-bit check
        assert_eq!(
            validate(&buf),
            Err(ParseError::ReservedBitSet(
                OFF_MAIN_MODE,
                buf[OFF_MAIN_MODE]
            ))
        );
    }
}
