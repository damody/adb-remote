## ADDED Requirements

### Requirement: Users can list online devices without an account
The user-side client SHALL allow any user who knows the server address to list currently online shared devices.

#### Scenario: List online devices
- **WHEN** the user runs `adb-remote devices --server <server-url>`
- **THEN** the client displays all online device display names with availability and password requirement

#### Scenario: No devices are online
- **WHEN** the server has no online registered devices
- **THEN** the client reports an empty device list without error

### Requirement: Password-protected devices require password validation
The server SHALL require password validation by the owning device client before granting a lease for a password-protected device.

#### Scenario: Correct password
- **WHEN** a user connects to a password-protected device and enters the correct password
- **THEN** the owning device client validates the password and the server grants the lease

#### Scenario: Incorrect password
- **WHEN** a user connects to a password-protected device and enters an incorrect password
- **THEN** the server rejects the lease with `InvalidPassword`

### Requirement: Passwordless devices can be leased without prompting
The server SHALL grant an available passwordless device lease without requesting a password.

#### Scenario: Connect passwordless device
- **WHEN** a user connects to an online passwordless device that is not leased
- **THEN** the server grants the lease without password validation

### Requirement: Device leases are exclusive
The server SHALL allow at most one active lease per online device.

#### Scenario: Available device is leased
- **WHEN** a user requests a lease for an online device with no active lease
- **THEN** the server creates a lease and marks the device as leased

#### Scenario: Leased device is requested by another user
- **WHEN** a second user requests a lease for a device that already has an active lease
- **THEN** the server rejects the request with `DeviceBusy`

### Requirement: Leases are released on disconnect or failure
The server SHALL release active leases when the user disconnects, the device client disconnects, heartbeat expires, or the server restarts.

#### Scenario: User disconnects
- **WHEN** the user runs `adb-remote disconnect <display-name>` or the user client session ends
- **THEN** the server releases the active lease for that user

#### Scenario: Device client disconnects
- **WHEN** the owning device client disconnects while a lease is active
- **THEN** the server releases the lease and marks the device offline

### Requirement: Password failures are rate limited
The server SHALL rate limit repeated password failures by source and display name.

#### Scenario: Repeated invalid passwords
- **WHEN** a source repeatedly submits invalid passwords for the same display name
- **THEN** the server delays or rejects further attempts according to the configured rate limit
