## 1. Rust Workspace Setup

- [x] 1.1 Create a Rust workspace with shared protocol/core crates and binaries for `adb-remote-server`, `adb-remote-client`, and `adb-remote`
- [x] 1.2 Add dependencies for `tokio`, `tonic`, `prost`, `rustls`, `clap`, SQLite, serialization, logging, and testing
- [x] 1.3 Add protobuf build configuration and generated Rust module wiring
- [x] 1.4 Add baseline CLI parsing and structured logging for all three binaries

## 2. Protocol Definitions

- [x] 2.1 Define protobuf messages for device registration, device snapshots, device list results, password validation, leases, audit metadata, and errors
- [x] 2.2 Define gRPC services for agent session, list devices, acquire lease, release lease, proxy ADB stream, and server status operations
- [x] 2.3 Implement shared Rust conversion types for protocol errors and domain state
- [x] 2.4 Add protocol-level tests for serialization and error mapping

## 3. Server Core

- [x] 3.1 Implement in-memory agent session and online device registry with active display name uniqueness checks
- [x] 3.2 Implement exclusive lease state machine with acquire, release, disconnect cleanup, and server restart invalidation behavior
- [x] 3.3 Implement password validation forwarding from server to owning device client
- [x] 3.4 Implement per-source and per-display-name password failure rate limiting
- [x] 3.5 Implement stream route tracking for active leases and stream ids
- [x] 3.6 Add unit tests for registry, lease transitions, password flow, rate limits, and route cleanup

## 4. Server Persistence And Console

- [x] 4.1 Add SQLite persistence for audit events and optional server configuration
- [x] 4.2 Record audit events for registration, offline transitions, lease start/end, invalid password, relay failure, and byte counts
- [x] 4.3 Implement `adb-remote-server init`
- [x] 4.4 Implement `adb-remote-server serve --listen <addr>` with TLS configuration
- [x] 4.5 Implement `adb-remote-server devices`, `leases`, and `audit`

## 5. Device Client

- [x] 5.1 Implement local config loading and saving for server URL, ADB path, configured serials, display names, password mode, and stored passwords
- [x] 5.2 Implement ADB executable discovery and `adb-remote-client scan`
- [x] 5.3 Implement `adb-remote-client add-device <serial> --name <display-name> --password|--no-password` with interactive password prompt
- [x] 5.4 Implement `adb-remote-client serve` agent gRPC session, heartbeat, and device snapshot reporting
- [x] 5.5 Implement local password validation request handling
- [x] 5.6 Implement `adb-remote-client status`

## 6. User Client

- [x] 6.1 Implement `adb-remote devices --server <server-url>` with display name, availability, and password requirement output
- [x] 6.2 Implement `adb-remote connect <display-name> --server <server-url> --local-port <port>` lease acquisition
- [x] 6.3 Implement interactive password prompt when the server reports password is required
- [x] 6.4 Implement local TCP listener startup after lease acquisition and lease release if binding fails
- [x] 6.5 Implement `adb-remote disconnect <display-name>` and `adb-remote leases`

## 7. ADB Relay

- [x] 7.1 Implement user-side local TCP connection to gRPC stream forwarding
- [x] 7.2 Implement server-side opaque stream relay between user stream and owning device client
- [x] 7.3 Implement device-side stream handling and TCP connection to local `127.0.0.1:5037`
- [x] 7.4 Implement close and error propagation in both directions
- [x] 7.5 Implement byte counting for audit metadata
- [x] 7.6 Add relay integration tests using a local TCP echo server instead of ADB

## 8. TLS And Configuration

- [x] 8.1 Implement TLS loading for server certificate/key and client trust configuration
- [x] 8.2 Add self-signed certificate documentation for internal testing
- [x] 8.3 Ensure server and clients fail clearly when required TLS configuration is missing
- [x] 8.4 Verify passwords and ADB payloads are not logged
- [x] 8.5 Add Linux server build, Docker, and systemd deployment assets

## 9. End-To-End Verification

- [x] 9.1 Add integration tests for mock device registration, device listing, successful lease, invalid password, busy device rejection, and release on disconnect
- [x] 9.2 Run formatting, linting, unit tests, and integration tests
- [ ] 9.3 Perform manual Windows test with a real Android device and `adb connect 127.0.0.1:<port>`
- [x] 9.4 Document MVP usage flow and known limitations
