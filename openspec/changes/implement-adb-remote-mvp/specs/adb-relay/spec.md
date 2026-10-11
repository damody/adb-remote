## ADDED Requirements

### Requirement: User client exposes leased device on a local TCP port
The user-side client SHALL open a local listener on `127.0.0.1:<local-port>` after acquiring a lease.

#### Scenario: Local port opens after lease
- **WHEN** the server grants a lease and the requested local port is available
- **THEN** the user client listens on `127.0.0.1:<local-port>` for ADB TCP connections

#### Scenario: Local port is unavailable
- **WHEN** the server grants a lease but the user client cannot bind the requested local port
- **THEN** the user client releases the lease and reports the bind failure

### Requirement: ADB connections are relayed as opaque bytes
The system SHALL relay each accepted local TCP connection as opaque byte streams between the user client, server, device client, and local ADB server.

#### Scenario: Local ADB connection opens
- **WHEN** a TCP client connects to the user client's local listener
- **THEN** the user client opens a gRPC ADB proxy stream with a lease id and stream id

#### Scenario: Bytes flow from user to device
- **WHEN** bytes are read from the local TCP connection
- **THEN** the bytes are sent through the gRPC relay to the device client and written to the device host's local ADB server

#### Scenario: Bytes flow from device to user
- **WHEN** bytes are read from the device host's local ADB server
- **THEN** the bytes are sent through the gRPC relay to the user client and written to the local TCP connection

### Requirement: Device client connects relay streams to local ADB server
The device-side client SHALL connect approved relay streams to `127.0.0.1:5037` on the device host.

#### Scenario: Local ADB server accepts stream
- **WHEN** the device client receives an approved stream open request
- **THEN** the device client opens a TCP connection to `127.0.0.1:5037` and relays bytes bidirectionally

#### Scenario: Local ADB server rejects stream
- **WHEN** the device client cannot connect to `127.0.0.1:5037`
- **THEN** the stream fails with `AdbUnavailable`

### Requirement: Relay streams close cleanly
The system SHALL propagate stream close and stream error events between both relay endpoints.

#### Scenario: User-side TCP closes
- **WHEN** the local TCP connection closes
- **THEN** the user client sends a stream close event and the device client closes its ADB TCP connection

#### Scenario: Device-side TCP closes
- **WHEN** the device host ADB TCP connection closes
- **THEN** the device client sends a stream close event and the user client closes its local TCP connection

### Requirement: Relay uses TLS-protected gRPC streams
All relay traffic SHALL travel over gRPC/TLS connections.

#### Scenario: TLS is not configured
- **WHEN** a server or client attempts to start relay communication without required TLS configuration
- **THEN** startup or connection fails with a clear TLS configuration error
