## Why

Internal Android device pools need a simple way to share ADB access across Windows machines without opening inbound ports on device hosts or replacing existing Android tooling. This change introduces a brokered ADB relay so developers can connect to a remote shared device through a local `adb connect 127.0.0.1:<port>` workflow.

## What Changes

- Add a Linux/Windows Rust gRPC/TLS server that acts as the public broker for device registration, device listing, exclusive leases, audit events, and opaque ADB byte relay.
- Add a Windows Rust device-side console client that scans local ADB devices, registers user-chosen display names, stores per-device passwords locally, validates password requests, and relays approved ADB streams to `127.0.0.1:5037`.
- Add a Windows Rust user-side console client that lists online devices, acquires an exclusive lease, prompts for a password when required, opens a local TCP listener, and forwards local ADB TCP connections through the server.
- Support passwordless and password-protected devices without server-side user accounts.
- Enforce display name uniqueness among active server registrations.
- Enforce one active lease per device.
- Record basic audit events without persisting ADB payloads or device passwords.

## Capabilities

### New Capabilities

- `remote-device-registry`: Device clients can register online ADB devices with user-chosen display names and availability metadata.
- `device-access-control`: Users can list shared devices, satisfy per-device password checks when required, and acquire exclusive leases.
- `adb-relay`: Leased devices can be exposed through a local TCP port and relayed as opaque ADB byte streams over gRPC/TLS.
- `audit-and-observability`: The server records operational audit events and exposes basic device, lease, and relay status through console commands.

### Modified Capabilities

None.

## Impact

- Adds a Rust workspace with Linux-capable server, Windows-focused device client, Windows-focused user client, shared protocol, and shared core crates or modules.
- Adds protobuf/gRPC service definitions and generated Rust bindings.
- Adds TLS configuration and certificate handling for internal deployments.
- Adds local device client configuration for ADB path, registered serials, display names, and per-device passwords.
- Adds SQLite persistence for server audit/config data and in-memory state for active sessions, devices, leases, and stream routing.
- Adds automated tests for registry behavior, password flow, lease state, and stream relay, plus manual Windows ADB validation.
