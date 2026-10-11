use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};
use tokio::net::TcpStream;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub details: String,
}

pub async fn check_adb_server() -> Result<()> {
    TcpStream::connect("127.0.0.1:5037")
        .await
        .context("connect local ADB server at 127.0.0.1:5037")?;
    Ok(())
}

pub fn scan_devices(adb_path: Option<&Path>) -> Result<Vec<AdbDevice>> {
    let adb = adb_path
        .map(|p| p.as_os_str().to_owned())
        .unwrap_or_else(|| "adb".into());
    let output = Command::new(adb)
        .args(["devices", "-l"])
        .output()
        .context("run adb devices -l")?;

    if !output.status.success() {
        return Err(anyhow!(
            "adb devices failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_adb_devices(&stdout))
}

pub fn parse_adb_devices(output: &str) -> Vec<AdbDevice> {
    output
        .lines()
        .skip_while(|line| !line.starts_with("List of devices"))
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next().unwrap_or("unknown").to_string();
            let details = parts.collect::<Vec<_>>().join(" ");
            Some(AdbDevice {
                serial,
                state,
                details,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adb_devices_l() {
        let output = "List of devices attached\r\nR5CT device product:x model:y\r\nemulator-5554 unauthorized\r\n";
        let devices = parse_adb_devices(output);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "R5CT");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[1].state, "unauthorized");
    }
}
