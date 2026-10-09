//! A small cache of the last `gw121 scan` results, so shell completion can offer recently seen addresses
//! without a multi-second BLE scan on every Tab press. Never used for anything but completion: `read`/`log`
//! always scan again before connecting (see `ble::connect`).

use serde_json::{Value, json};
use std::path::PathBuf;

fn path() -> PathBuf {
    let dir = std::env::var("GW121_CLI_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("gw121_cli")
        });
    dir.join("last-scan.json")
}

pub fn save_scan(found: &[crate::ble::Found]) {
    let devices: Vec<Value> = found
        .iter()
        .map(|f| {
            json!({
                "address": f.address.to_string(),
                "name": f.name,
                "rssi": f.rssi,
            })
        })
        .collect();
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_vec(&devices).unwrap_or_default());
}

/// (address, name, rssi) for every device in the last saved scan; empty if there isn't one yet.
pub fn load_scan() -> Vec<(String, Option<String>, Option<i16>)> {
    let Ok(bytes) = std::fs::read(path()) else {
        return Vec::new();
    };
    let Ok(Value::Array(items)) = serde_json::from_slice(&bytes) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|v| {
            let address = v.get("address")?.as_str()?.to_string();
            let name = v.get("name").and_then(|n| n.as_str()).map(str::to_string);
            let rssi = v.get("rssi").and_then(|r| r.as_i64()).map(|r| r as i16);
            Some((address, name, rssi))
        })
        .collect()
}
