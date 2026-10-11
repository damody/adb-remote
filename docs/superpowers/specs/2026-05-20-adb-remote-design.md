# ADB Remote Design

Date: 2026-05-20

## Purpose

Build an internal ADB remote access tool for a company device pool. A Windows machine connected to Android devices can share those devices through a public or reachable server, which may run on Linux or Windows. Another Windows machine can list shared devices, lease one exclusively, and expose it locally so existing ADB tooling can connect with `adb connect 127.0.0.1:<port>`.

The system favors simple deployment, reliable NAT/firewall traversal, and compatibility with existing Android tooling.

## Architecture

The MVP has three Rust console programs:

- `adb-remote-server`: Linux/Windows public broker with a reachable IP. It manages online device state, display name uniqueness, exclusive leases, audit events, and gRPC stream relay. It does not install Android SDK and does not directly speak to ADB.
- `adb-remote-client`: device-side console daemon for Windows hosts connected to Android devices. It scans local ADB devices, registers configured devices with the server, validates per-device passwords, and relays approved ADB streams to the local ADB server at `127.0.0.1:5037`.
- `adb-remote`: user-side console program. It lists devices from the server, leases one display name, opens a local TCP listener, and forwards local ADB TCP bytes through the server to the device client.

Both device-side and user-side programs initiate outbound connections to the server. No Windows client machine needs an inbound firewall rule.

## Transport

Use gRPC over TLS for both control and ADB data relay.

Rust libraries:

- `tokio` for async runtime and TCP forwarding.
- `tonic` and `prost` for gRPC/protobuf.
- `rustls` for TLS.
- `clap` for console commands.

The server must require TLS for MVP because device passwords and ADB traffic pass through the relay.

## Device Registration

Each shared Android device has:

- `device_id`: server-generated runtime identifier.
- `display_name`: user-chosen name configured on the device host, unique among active server registrations.
- `adb_serial`: serial shown by local `adb devices -l`.
- `agent_id`: identifies the connected `adb-remote-client` session.
- `password_required`: whether the user must enter a password before leasing.
- optional `group` and `tags` metadata for later filtering.

`display_name` is chosen by the device-side user. If a new registration conflicts with an existing active registration, the server rejects it with `NameAlreadyExists`, and the device client asks the user to choose another name.

## Password Model

The server has no user accounts in the MVP. Anyone who knows the server address can run `adb-remote devices` and see all online display names.

Device access is controlled per device:

- A device may be configured with no password.
- A device may be configured with a password stored only by `adb-remote-client`.
- The server does not persist, log, or hash device passwords.

Connection flow for password-protected devices:

1. User runs `adb-remote connect <display-name>`.
2. Server reports that a password is required.
3. `adb-remote` prompts for the password.
4. The password is sent over gRPC/TLS to the server.
5. The server forwards it to the owning `adb-remote-client`.
6. The device client validates it locally.
7. On success, the server creates an exclusive lease. On failure, the server returns `InvalidPassword` and applies rate limiting.

This direct password forwarding is an MVP simplification. A later version may replace it with challenge-response without changing the user workflow.

## Lease Model

Each device allows only one active lease.

Lease flow:

1. User runs `adb-remote devices`.
2. User selects a `display_name`.
3. User runs `adb-remote connect <display-name> --local-port <port>`.
4. Server checks the device is online and not leased.
5. If required, password validation is delegated to the device client.
6. Server creates a `lease_id`.
7. User-side `adb-remote` starts listening on `127.0.0.1:<port>`.
8. User runs `adb connect 127.0.0.1:<port>`.
9. Every local ADB TCP connection becomes a proxied gRPC stream.

Lease release happens when:

- User runs `adb-remote disconnect <display-name>`.
- User-side console exits or disconnects.
- Device client heartbeat times out.
- Relay stream fails.
- Server restarts.

MVP does not support queueing, reservation, or shared multi-user access.

## gRPC API Shape

The exact protobuf may evolve during implementation, but MVP should include these service areas:

- `RegisterAgent` or `AgentSession`: device client establishes a long-lived bidirectional stream.
- `DeviceSnapshot`: device client reports configured and currently available ADB devices.
- `ValidatePassword`: server asks the device client to validate a password for a display name.
- `ListDevices`: user console lists all online devices and their availability.
- `AcquireLease`: user console requests exclusive access.
- `ReleaseLease`: user console releases access.
- `ProxyAdbStream`: bidirectional stream carrying ADB bytes for one local TCP connection.

ADB stream frames should include:

- `lease_id`
- `stream_id`
- `open`
- `data`
- `close`
- `error`

The server treats ADB payloads as opaque bytes. It records metadata such as byte counts and error reasons, but never parses ADB protocol contents.

## Console Commands

Device-side:

```powershell
adb-remote-client scan
adb-remote-client add-device <adb-serial> --name <display-name> --password
adb-remote-client add-device <adb-serial> --name <display-name> --no-password
adb-remote-client serve --server https://<server-ip>:<port>
adb-remote-client status
```

User-side:

```powershell
adb-remote devices --server https://<server-ip>:<port>
adb-remote connect <display-name> --server https://<server-ip>:<port> --local-port 5555
adb-remote disconnect <display-name>
adb-remote leases
```

Server-side:

```powershell
adb-remote-server init
adb-remote-server serve --listen 0.0.0.0:8443
adb-remote-server devices
adb-remote-server leases
adb-remote-server audit
```

No `create-user` command exists in MVP.

When `--password` is used, the device client prompts for the password interactively instead of accepting it as a command-line value. This avoids storing the password in shell history.

## Server Storage

Use SQLite for persistent server-side data:

- audit events
- optional server configuration
- recent known device metadata, if useful for diagnostics

Runtime state can remain in memory:

- active agent sessions
- online devices
- active leases
- stream routing table
- password failure rate limit buckets

After a server restart, all active leases are invalid. Device clients and user consoles must reconnect.

## Device Client Behavior

`adb-remote-client` is a console daemon for MVP, not a Windows Service.

Startup behavior:

1. Load local config.
2. Locate `adb.exe` from explicit config first, then `PATH`.
3. Check local ADB server at `127.0.0.1:5037`.
4. Run `adb devices -l` or equivalent scan.
5. Match configured `adb_serial` values to local devices.
6. Open an agent gRPC stream to the server.
7. Report device snapshots and heartbeat updates.
8. Respond to password validation and stream relay requests.

If a configured `adb_serial` is missing, the device is reported unavailable rather than registered as online.

## User Client Behavior

`adb-remote` is a console program.

For `connect`, it:

1. Requests a lease by display name.
2. Prompts for a password if required.
3. Starts a local TCP listener on `127.0.0.1:<local-port>`.
4. For each inbound local TCP connection, opens a gRPC ADB proxy stream.
5. Copies bytes bidirectionally until either side closes.
6. Releases the lease when the command exits or `disconnect` is called.

The intended user workflow is:

```powershell
adb-remote devices --server https://10.0.0.10:8443
adb-remote connect qa-pixel-8-01 --server https://10.0.0.10:8443 --local-port 5555
adb connect 127.0.0.1:5555
adb devices
```

## Error Handling

Expected errors:

- `NameAlreadyExists`: display name is already online.
- `DeviceOffline`: device client is disconnected or device serial is missing.
- `DeviceBusy`: another lease is active.
- `PasswordRequired`: user must enter password before acquiring lease.
- `InvalidPassword`: device client rejected the password.
- `AdbUnavailable`: device client cannot connect to local ADB server.
- `LeaseExpired`: heartbeat or stream failure invalidated the lease.
- `RelayFailed`: stream setup or forwarding failed.

Cleanup rules:

- If user console disconnects, server releases the lease and closes related streams.
- If device client disconnects, server marks devices offline and releases leases.
- If a relay stream closes, only that stream closes; the lease remains active unless the control connection also fails.
- If the local ADB listener fails to bind, no lease should remain active.

## Security

MVP security requirements:

- gRPC must use TLS.
- Server must never log device passwords.
- Server must never persist device passwords.
- Password failure attempts are rate limited per source IP and display name.
- Audit events record source IP, optional hostname, display name, lease timing, byte counts, and failure reasons.
- ADB payloads are opaque and are not written to disk.

Because there is no account system, knowing the server address allows viewing device names. This is accepted for MVP.

## MVP Scope

Included:

- Rust console programs with Linux/Windows server support and Windows-focused device/user clients.
- gRPC/TLS server, device client, and user client.
- Device scan and local config.
- User-chosen display names that are unique among active server registrations.
- Password-protected and passwordless devices.
- Server-visible online device list.
- Exclusive leases.
- Local TCP listener compatible with `adb connect`.
- Opaque ADB byte relay.
- Basic audit events.
- Rate limiting for password failures.

Not included:

- User accounts or roles.
- Web UI.
- Windows Service installation.
- P2P direct relay.
- Queueing or reservations.
- Multi-user shared access to one device.
- ADB protocol parsing.
- Cross-platform device/user client support beyond Windows.
- AD/LDAP/SSO integration.

## Testing Strategy

Unit tests:

- display name registry
- lease state transitions
- password required / no password behavior
- rate limit decisions
- stream routing table cleanup

Integration tests:

- mock device agent registers devices
- mock user lists devices
- successful lease
- invalid password
- busy device rejection
- release on disconnect

Relay tests:

- use a local TCP echo server instead of ADB
- verify bidirectional byte forwarding through gRPC streams
- verify close propagation
- verify byte counters

Manual Windows test:

1. Start `adb-remote-server`.
2. Connect an Android device to a Windows device host.
3. Run `adb-remote-client scan`.
4. Add the serial with a display name and optional password.
5. Run `adb-remote-client serve`.
6. On another Windows machine, run `adb-remote devices`.
7. Run `adb-remote connect <display-name> --local-port 5555`.
8. Run `adb connect 127.0.0.1:5555`.
9. Confirm `adb devices`, `adb shell`, and `adb install` work.

## Open Implementation Notes

- Decide during implementation whether one binary with subcommands or three separate binaries is cleaner. The product names remain as specified either way.
- Define certificate setup for MVP. Self-signed certificates are acceptable for internal testing if documented clearly.
- Keep protobuf boundaries small and explicit so later challenge-response authentication can replace direct password forwarding without changing console commands.
