# ADB Remote MVP Usage

This MVP provides three console binaries:

- `adb-remote-server`
- `adb-remote-client`
- `adb-remote`

`adb-remote-server` supports Linux and Windows. The device-side and user-side clients are currently focused on Windows.

All network communication uses gRPC over TLS. The server never persists device passwords and never stores ADB payload bytes.

## Build

```powershell
cargo build
```

## Build Installer

With NSIS installed:

```powershell
.\scripts\build-installer.ps1 -Version 0.1.0
```

The installer is written to:

```powershell
dist\adb-remote-0.1.0-windows-x64-setup.exe
```

The installer places the binaries in `C:\Program Files\ADB Remote`. It does not modify the system `PATH`; add that install directory to `PATH` if you want to run the commands from any terminal.

## Linux Server

For production server deployment on Linux, see [server-linux.md](server-linux.md).

## Create A Self-Signed Certificate For Testing

For local internal testing, create a certificate with Subject Alternative Names for the host name or IP that clients use.

Example with OpenSSL:

```powershell
openssl req -x509 -newkey rsa:4096 -nodes `
  -keyout server.key `
  -out server.pem `
  -days 365 `
  -subj "/CN=localhost" `
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
```

Use `server.pem` as the client CA file for testing.

## Start Server

```powershell
cargo run -p adb-remote-server -- init --db adb-remote-server.sqlite3
cargo run -p adb-remote-server -- serve --listen 0.0.0.0:8443 --db adb-remote-server.sqlite3 --cert server.pem --key server.key
```

## Configure Device Host

On the Windows machine with Android devices attached:

```powershell
cargo run -p adb-remote-client -- scan
cargo run -p adb-remote-client -- add-device <adb-serial> --name qa-pixel-8-01 --password
cargo run -p adb-remote-client -- serve --server https://localhost:8443 --ca server.pem --domain localhost
```

Use `--no-password` instead of `--password` for an open device.

## Connect From User Machine

```powershell
cargo run -p adb-remote -- devices --server https://localhost:8443 --ca server.pem --domain localhost
cargo run -p adb-remote -- connect qa-pixel-8-01 --server https://localhost:8443 --ca server.pem --domain localhost --local-port 5555
adb connect 127.0.0.1:5555
adb devices
```

Press `Ctrl+C` in the `adb-remote connect` process to release the lease.

## Operational Commands

```powershell
cargo run -p adb-remote-server -- devices --server https://localhost:8443 --ca server.pem --domain localhost
cargo run -p adb-remote-server -- leases --server https://localhost:8443 --ca server.pem --domain localhost
cargo run -p adb-remote-server -- audit --db adb-remote-server.sqlite3
cargo run -p adb-remote-client -- status
cargo run -p adb-remote -- leases --server https://localhost:8443 --ca server.pem --domain localhost
```

## Known MVP Limitations

- No user accounts; anyone who knows the server URL can list online display names.
- Device passwords are validated by the device client but pass through server memory over TLS.
- Active display names are unique only among active registrations.
- Server restart invalidates all leases.
- No queueing, reservation, P2P relay, Web UI, or Windows Service installer.

## Manual Test Helper

After creating a test certificate and connecting a real Android device, you can run:

```powershell
.\scripts\manual-adb-test.ps1
```

The script builds the binaries, guides you through starting the server and device client in separate windows, opens a user tunnel, runs `adb connect`, and performs a small `adb shell getprop` smoke test.
