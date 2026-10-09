//! The command tree (clap derive) and the command handlers; `--json` output goes through [`Out::print`].

use crate::protocol::{Flags, Packet, Reading};
use crate::reassemble::Reassembler;
use crate::{ble, complete, parse, state};
use anyhow::{Context as _, Result, anyhow};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::ArgValueCandidates;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde_json::{Value, json};
use std::io::{IsTerminal, Write as _};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("GIT_SHA"),
    ", built ",
    env!("BUILD_TIME"),
    ")"
);

#[derive(Parser, Debug)]
#[command(name = "gw121", version = VERSION, about, long_about = "\
Read an EEVblog 121GW multimeter over Bluetooth LE: the main and secondary displays, the bargraph, and the \
meter's flags (auto-range, hold, relative, AC/DC, battery). Mode and range live on the meter's knob and are \
only ever read, never set.

Hold the 1ms PEAK button until the BT indicator lights to put the meter into BLE mode. Typical use: \
`gw121 scan`, then `gw121 read` or `gw121 log --for 1m --csv out.csv`. Shell completion: \
`source <(COMPLETE=bash gw121)` (or zsh, fish).")]
struct Cli {
    /// Machine-readable JSON on stdout (one object per line).
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Scan for 121GW meters over BLE.
    Scan {
        /// How long to scan.
        #[arg(long, default_value = "5s", value_parser = parse::duration)]
        timeout: Duration,
        /// Also list other BLE devices seen while scanning, not just ones named "121GW".
        #[arg(long)]
        all: bool,
    },
    /// Connect and print one reading from the main and secondary displays, the bargraph, and the flags.
    Read {
        /// BLE address (88:6B:...); without it, scans and uses the one 121GW found.
        #[arg(long, short = 'a', env = "GW121_ADDRESS", add = ArgValueCandidates::new(complete::address_candidates))]
        address: Option<String>,
        /// Scan and connect timeout.
        #[arg(long, default_value = "10s", value_parser = parse::duration)]
        timeout: Duration,
    },
    /// Connect and print readings continuously until Ctrl+C or --for.
    Log {
        /// BLE address (88:6B:...); without it, scans and uses the one 121GW found.
        #[arg(long, short = 'a', env = "GW121_ADDRESS", add = ArgValueCandidates::new(complete::address_candidates))]
        address: Option<String>,
        /// Scan and connect timeout.
        #[arg(long, default_value = "10s", value_parser = parse::duration)]
        timeout: Duration,
        /// Stop after this long (default: run until Ctrl+C).
        #[arg(long = "for", value_parser = parse::duration)]
        for_: Option<Duration>,
        /// Print at most one line this often (default: print every packet, about 2/s).
        #[arg(long, value_parser = parse::duration)]
        every: Option<Duration>,
        /// Also write every line as CSV to this file (created or appended to).
        #[arg(long)]
        csv: Option<PathBuf>,
    },
}

/// Output of a command: text for people, or JSON with `--json`.
struct Out {
    json: bool,
}

impl Out {
    fn print(&self, text: impl AsRef<str>, value: Value) {
        if self.json {
            println!("{value}");
        } else {
            let text = text.as_ref();
            if !text.is_empty() {
                println!("{}", text.trim_end());
            }
        }
    }

    /// A warning on stderr (text mode only; in JSON it is part of the value).
    fn warn(&self, text: &str) {
        if !self.json {
            eprintln!("{} {text}", style::yellow("warning:"));
        }
    }

    fn spinner(&self, msg: &str) -> ProgressBar {
        let bar = ProgressBar::new_spinner();
        if self.json || !std::io::stderr().is_terminal() {
            bar.set_draw_target(ProgressDrawTarget::hidden());
        }
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        bar.enable_steady_tick(Duration::from_millis(100));
        bar.set_message(msg.to_string());
        bar
    }
}

/// Colour on stdout/stderr when it is a terminal and `NO_COLOR` is not set.
mod style {
    use std::io::IsTerminal;
    use std::sync::OnceLock;

    fn on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| {
            std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
                && std::io::stderr().is_terminal()
                && std::io::stdout().is_terminal()
        })
    }

    fn paint(code: &str, text: &str) -> String {
        if on() {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn dim(t: &str) -> String {
        paint("2", t)
    }
    pub fn yellow(t: &str) -> String {
        paint("33", t)
    }
    pub fn red(t: &str) -> String {
        paint("31", t)
    }
}

pub fn main() {
    complete::complete(Cli::command);
    let cli = Cli::parse();
    let json = cli.json;
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("{} could not start: {e:#}", style::red("error:"));
            std::process::exit(1);
        }
    };
    if let Err(e) = rt.block_on(run(cli)) {
        if json {
            println!("{}", json!({ "error": format!("{e:#}") }));
        } else {
            eprintln!("{} {e:#}", style::red("error:"));
        }
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    let out = Out { json: cli.json };
    match cli.cmd {
        Cmd::Scan { timeout, all } => cmd_scan(&out, timeout, all).await,
        Cmd::Read { address, timeout } => cmd_read(&out, address, timeout).await,
        Cmd::Log {
            address,
            timeout,
            for_,
            every,
            csv,
        } => cmd_log(&out, address, timeout, for_, every, csv).await,
    }
}

async fn cmd_scan(out: &Out, timeout: Duration, all: bool) -> Result<()> {
    let adapter = ble::adapter().await.context("opening the BLE adapter")?;
    let spinner = out.spinner("scanning for BLE devices...");
    let filter = if all { "" } else { "121gw" };
    let found = ble::scan(&adapter, timeout, filter, false)
        .await
        .context("scanning")?;
    spinner.finish_and_clear();
    state::save_scan(&found);
    if found.is_empty() {
        out.warn(if all {
            "no BLE devices seen; is Bluetooth on?"
        } else {
            "no 121GW seen; hold the meter's 1ms PEAK button until its BT indicator lights, then try again"
        });
    }
    for f in &found {
        let name = f.name.clone().unwrap_or_else(|| "-".to_string());
        let rssi = match (f.rssi, f.connected) {
            (Some(r), _) => format!("{r} dBm"),
            (None, true) => "connected".to_string(),
            (None, false) => "not heard (remembered by BlueZ)".to_string(),
        };
        out.print(
            format!("{:<20} {:<16} {}", f.address, name, style::dim(&rssi)),
            json!({ "address": f.address.to_string(), "name": f.name, "rssi": f.rssi, "connected": f.connected }),
        );
    }
    Ok(())
}

/// Resolves `--address`, scanning for the one 121GW nearby when it was not given.
async fn resolve_address(
    out: &Out,
    adapter: &bluer::Adapter,
    address: Option<String>,
    timeout: Duration,
) -> Result<bluer::Address> {
    if let Some(a) = address {
        return a
            .parse()
            .map_err(|_| anyhow!("`{a}` is not a BLE address (expected 88:6B:0F:73:00:01)"));
    }
    let spinner = out.spinner("scanning for a 121GW...");
    let found = ble::scan(adapter, timeout, "121gw", true)
        .await
        .context("scanning")?;
    spinner.finish_and_clear();
    state::save_scan(&found);
    match found.len() {
        0 => Err(anyhow!(
            "no 121GW seen in {timeout:?}; hold its 1ms PEAK button until the BT indicator lights, or pass --address"
        )),
        1 => Ok(found[0].address),
        _ => {
            let list = found
                .iter()
                .map(|f| f.address.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(anyhow!(
                "more than one 121GW nearby ({list}); pass --address to pick one"
            ))
        }
    }
}

async fn connect_characteristic(
    out: &Out,
    address: bluer::Address,
    timeout: Duration,
) -> Result<(bluer::Device, bluer::gatt::remote::Characteristic)> {
    let adapter = ble::adapter().await.context("opening the BLE adapter")?;
    let spinner = out.spinner(&format!("connecting to {address}..."));
    // BlueZ keeps a meter it saw earlier as a device object and its connect() waits on its own, much longer
    // timeout when the meter is off: bound the whole connect + GATT lookup by --timeout.
    let connect = async {
        let device = ble::connect(&adapter, address, timeout)
            .await
            .context("connecting")?;
        let characteristic = ble::find_data_characteristic(&device)
            .await
            .context("looking for the meter's data characteristic")?;
        Ok::<_, anyhow::Error>((device, characteristic))
    };
    let connected = tokio::time::timeout(timeout, connect)
        .await
        .map_err(|_| {
            anyhow!(
                "could not connect to {address} in {timeout:?}; is the meter on, in BLE mode and in range?"
            )
        })??;
    spinner.finish_and_clear();
    Ok(connected)
}

/// Runs `body` on the meter's data characteristic, then disconnects: a meter left connected stops
/// advertising, so the next scan would not hear it.
async fn with_meter<T>(
    out: &Out,
    address: bluer::Address,
    timeout: Duration,
    body: impl AsyncFnOnce(&bluer::gatt::remote::Characteristic) -> Result<T>,
) -> Result<T> {
    let (device, characteristic) = connect_characteristic(out, address, timeout).await?;
    let result = body(&characteristic).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), device.disconnect()).await;
    result
}

fn reading_json(r: &Result<Reading, crate::protocol::ParseError>) -> Value {
    match r {
        Ok(r) => json!({
            "value": if r.value.is_finite() { json!(round_to(r.value, r.digits)) } else { json!(if r.value > 0.0 {"inf"} else {"-inf"}) },
            "unit": r.unit.symbol(),
            "quantity": format!("{:?}", r.quantity).to_lowercase(),
            "digits": r.digits,
            "ac": r.ac,
            "dc": r.dc,
            "overload": r.overload,
        }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

/// The value with the meter's decimals only, so JSON shows 3.2971, not 3.2971000000000004.
/// `digits` may be negative (whole kilohms of a megohm range).
fn round_to(value: f64, digits: i32) -> f64 {
    let scale = 10f64.powi(digits);
    (value * scale).round() / scale
}

fn flags_json(f: &Flags) -> Value {
    json!({
        "auto_range": f.auto_range,
        "auto_power_off": f.auto_power_off,
        "low_battery": f.low_battery,
        "bluetooth": f.bluetooth,
        "hold": f.hold,
        "relative": f.relative,
        "test": f.test,
        "memory": f.memory,
        "low_pass": f.low_pass,
        "peak_1ms": f.peak_1ms,
    })
}

fn packet_json(p: &Packet) -> Value {
    let mut v = json!({});
    let obj = v.as_object_mut().unwrap();
    obj.insert("main".to_string(), reading_json(&p.main()));
    obj.insert("sub".to_string(), reading_json(&p.sub()));
    match p.bar() {
        Ok(Some(bar)) => {
            obj.insert(
                "bar".to_string(),
                json!({ "value": round_to(bar.value, bar.digits), "digits": bar.digits }),
            );
        }
        Ok(None) => {}
        Err(e) => {
            obj.insert("bar".to_string(), json!({ "error": e.to_string() }));
        }
    }
    obj.insert("flags".to_string(), flags_json(&p.flags()));
    v
}

fn packet_text(p: &Packet) -> String {
    let mut lines = Vec::new();
    match p.main() {
        Ok(r) => {
            let acdc = match (r.ac, r.dc) {
                (true, true) => " AC+DC",
                (true, false) => " AC",
                (false, true) => " DC",
                (false, false) => "",
            };
            lines.push(format!("main: {}{acdc}", parse::fmt_reading(&r)));
        }
        Err(e) => lines.push(format!("main: -- ({e})")),
    }
    if let Ok(r) = p.sub() {
        lines.push(format!("sub:  {}", parse::fmt_reading(&r)));
    }
    if let Ok(Some(r)) = p.bar() {
        lines.push(format!("bar:  {}", parse::fmt_reading(&r)));
    }
    let f = p.flags();
    let mut tags = Vec::new();
    if f.auto_range {
        tags.push("AUTO");
    }
    if f.hold {
        tags.push("HOLD");
    }
    if f.relative {
        tags.push("REL");
    }
    if f.low_battery {
        tags.push("LOW-BATT");
    }
    if !tags.is_empty() {
        lines.push(tags.join(" "));
    }
    lines.join("\n")
}

async fn cmd_read(out: &Out, address: Option<String>, timeout: Duration) -> Result<()> {
    let adapter = ble::adapter().await.context("opening the BLE adapter")?;
    let address = resolve_address(out, &adapter, address, timeout).await?;
    with_meter(out, address, timeout, async |c| {
        read_one(out, address, timeout, c).await
    })
    .await
}

async fn read_one(
    out: &Out,
    address: bluer::Address,
    timeout: Duration,
    characteristic: &bluer::gatt::remote::Characteristic,
) -> Result<()> {
    let mut stream = ble::notify_stream(characteristic)
        .await
        .context("subscribing")?;
    let mut reassembler = Reassembler::new();
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            anyhow::bail!("no valid packet from {address} in {timeout:?}");
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let chunk = match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => anyhow::bail!("{address} disconnected (meter off or out of range)"),
            Err(_) => anyhow::bail!("no valid packet from {address} in {timeout:?}"),
        };
        reassembler.push(&chunk);
        if let Some(buf) = reassembler.next_packet() {
            let packet = Packet::parse(buf).context("validated but unparseable packet")?;
            out.print(packet_text(&packet), packet_json(&packet));
            return Ok(());
        }
    }
}

async fn cmd_log(
    out: &Out,
    address: Option<String>,
    timeout: Duration,
    for_: Option<Duration>,
    every: Option<Duration>,
    csv: Option<PathBuf>,
) -> Result<()> {
    let adapter = ble::adapter().await.context("opening the BLE adapter")?;
    let address = resolve_address(out, &adapter, address, timeout).await?;
    with_meter(out, address, timeout, async |c| {
        log_stream(out, address, c, for_, every, csv).await
    })
    .await
}

async fn log_stream(
    out: &Out,
    address: bluer::Address,
    characteristic: &bluer::gatt::remote::Characteristic,
    for_: Option<Duration>,
    every: Option<Duration>,
    csv: Option<PathBuf>,
) -> Result<()> {
    let mut stream = ble::notify_stream(characteristic)
        .await
        .context("subscribing")?;
    let mut reassembler = Reassembler::new();

    let mut csv_file = match &csv {
        Some(path) => {
            let is_new = !path.exists();
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            if is_new {
                writeln!(
                    f,
                    "timestamp_ms,main_value,main_unit,main_ac,main_dc,main_overload,sub_value,sub_unit,bar_value,auto_range,hold,relative,low_battery"
                )?;
            }
            Some(f)
        }
        None => None,
    };

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || r.store(false, Ordering::SeqCst))
        .context("installing the Ctrl+C handler")?;

    let start = Instant::now();
    let mut last_printed: Option<Instant> = None;
    while running.load(Ordering::SeqCst) {
        if let Some(for_) = for_
            && start.elapsed() >= for_
        {
            break;
        }
        // Wait at most 2 s, and never past the end of --for.
        let mut wait = Duration::from_secs(2);
        if let Some(for_) = for_ {
            wait = wait.min(for_.saturating_sub(start.elapsed()));
        }
        let chunk = match tokio::time::timeout(wait, stream.next()).await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => anyhow::bail!("{address} disconnected (meter off or out of range)"),
            Err(_) => {
                let done = for_.is_some_and(|f| start.elapsed() >= f);
                if running.load(Ordering::SeqCst) && !done {
                    out.warn(&format!("no data from {address} for 2s; still listening"));
                }
                continue;
            }
        };
        reassembler.push(&chunk);
        while let Some(buf) = reassembler.next_packet() {
            let now = Instant::now();
            if let Some(every) = every
                && last_printed.is_some_and(|t| now.duration_since(t) < every)
            {
                continue;
            }
            last_printed = Some(now);
            let packet = Packet::parse(buf).context("validated but unparseable packet")?;
            out.print(packet_text(&packet), packet_json(&packet));
            if let Some(f) = csv_file.as_mut() {
                write_csv_row(f, &packet)?;
            }
        }
    }
    Ok(())
}

fn write_csv_row(f: &mut std::fs::File, p: &Packet) -> Result<()> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let (mv, mu, mac, mdc, mof) = match p.main() {
        Ok(r) => (
            parse::fmt_reading(&r),
            r.unit.symbol().to_string(),
            r.ac,
            r.dc,
            r.overload,
        ),
        Err(_) => (String::new(), String::new(), false, false, false),
    };
    let (sv, su) = match p.sub() {
        Ok(r) => (parse::fmt_reading(&r), r.unit.symbol().to_string()),
        Err(_) => (String::new(), String::new()),
    };
    let bv = match p.bar() {
        Ok(Some(r)) => parse::fmt_reading(&r),
        _ => String::new(),
    };
    let flags = p.flags();
    writeln!(
        f,
        "{ts},{mv},{mu},{mac},{mdc},{mof},{sv},{su},{bv},{},{},{},{}",
        flags.auto_range, flags.hold, flags.relative, flags.low_battery
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::round_to;

    #[test]
    fn round_to_meter_digits() {
        assert_eq!(round_to(3.2971000000000004, 4), 3.2971);
        assert_eq!(round_to(1234.0, -3), 1000.0);
    }
}
