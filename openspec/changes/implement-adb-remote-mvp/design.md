## Context

The repository currently contains project configuration and a product design spec for an ADB remote access tool. The MVP targets company-internal device pools where Android devices are physically connected to Windows hosts, other Windows users need to access those devices through their normal local ADB workflow, and the broker server commonly runs on Linux.

The system has no user account model in the MVP. Anyone who knows the server address can list online devices. Access control is per-device: a shared device can be passwordless or password-protected, and password validation is performed by the owning device client.

## Goals / Non-Goals

**Goals:**

- Provide three Rust console programs: Linux/Windows `adb-remote-server`, Windows-focused `adb-remote-client`, and Windows-focused `adb-remote`.
- Let device-side Windows clients register local ADB devices with user-chosen display names.
- Let user-side Windows clients list online devices, acquire an exclusive lease, and expose the device through a local TCP port compatible with `adb connect`.
- Relay ADB byte streams over gRPC/TLS without parsing ADB protocol payloads.
- Store device passwords only on the device-side client.
- Keep server state simple: persistent audit/config data in SQLite and runtime sessions/leases in memory.

**Non-Goals:**

- User accounts, roles, AD/LDAP/SSO, or server-side authentication.
- Web UI or desktop GUI.
- Windows Service installation.
- P2P direct relay.
- Queueing, reservation, or shared multi-user access to one device.
- Cross-platform device/user client support beyond Windows.
- ADB protocol parsing.

## Decisions

### Use server-brokered relay

Both user-side and device-side clients initiate outbound gRPC/TLS connections to `adb-remote-server`. The server owns online device state, exclusive leases, stream routing, and audit metadata.

Alternatives considered:

- P2P data plane: lower server bandwidth but fragile with Windows firewalls, NAT, and segmented company networks.
- VPN/overlay: closest to LAN behavior but adds driver/networking complexity that is too heavy for the MVP.

The brokered relay is the best MVP fit because it minimizes Windows networking setup and keeps failure modes centralized.

### Use gRPC/TLS for control and data

Control operations and ADB proxy streams use typed protobuf messages over gRPC/TLS. ADB payloads are carried as opaque bytes in stream frames.

Alternatives considered:

- WebSocket with JSON/binary frames: easy to inspect, but weaker typed contracts and more custom protocol code.
- Raw TCP: efficient, but requires more custom framing, authentication, and operational tooling.

gRPC/TLS provides typed contracts, bidirectional streaming, backpressure support, and a straightforward Rust stack through `tonic`, `prost`, `tokio`, and `rustls`.

### Keep passwords on device clients

The server never persists, hashes, or logs device passwords. For protected devices, the user client sends the entered password over TLS to the server, and the server forwards it to the owning device client for validation.

Alternatives considered:

- Server-side password hashes: simpler validation, but expands server trust and persistence responsibilities.
- Challenge-response: better password exposure properties, but more implementation complexity than the MVP needs.

Direct forwarding over TLS is accepted for MVP and can later be replaced by challenge-response without changing the console workflow.

### Enforce exclusive leases

Each display name can have at most one active lease. The server rejects concurrent lease attempts with `DeviceBusy` and releases leases on explicit disconnect, user disconnect, device disconnect, heartbeat timeout, or server restart.

Alternatives considered:

- Multi-user shared ADB: simpler server state but unsafe because concurrent ADB operations interfere with one another.
- Queueing/reservation: useful later, but not required for first working access.

### Store runtime state in memory

SQLite stores audit events and optional server configuration. Online devices, connected agents, active leases, stream routes, and rate-limit buckets live in memory.

This keeps the MVP implementation clear. Server restart invalidates leases, and clients reconnect to rebuild runtime state.

## Risks / Trade-offs

- Server bandwidth bottleneck -> Keep payload handling streaming and opaque; defer P2P or relay sharding until there is measured pressure.
- Passwords transit through server memory -> Require TLS, never log passwords, keep validation forwarding narrowly scoped, and document challenge-response as a future upgrade.
- Anyone with server address can list display names -> Accept for MVP; password-protect sensitive devices and add optional server access controls later if needed.
- gRPC stream flow control can affect large APK installs or long logcat sessions -> Use async backpressure-aware forwarding, set explicit message limits, and test with large payloads.
- Server restart drops leases -> Treat runtime leases as ephemeral and require user/device clients to reconnect.
- Display names are only unique among active registrations -> Return `NameAlreadyExists` for active conflicts and make device-side rename flow explicit.

## Migration Plan

This is a new product with no existing runtime migration. Implementation should start with a new Rust workspace, protocol definitions, console binaries, Windows installer assets, and Linux server deployment assets.

Rollback for internal testing is to stop the server and clients. No existing ADB setup is modified; users can continue using local ADB directly.

## Open Questions

- Whether to ship as three binaries or one binary with subcommands can be decided during implementation.
- Exact certificate bootstrap for internal testing needs documentation; self-signed certificates are acceptable for MVP.
- Long-term password flow may move to challenge-response after the direct TLS forwarding MVP is stable.
