## ADDED Requirements

### Requirement: Server records audit events
The server SHALL persist audit events for device registration, device offline, lease creation, lease release, password failure, and relay failure.

#### Scenario: Device registration is audited
- **WHEN** a device is registered online
- **THEN** the server stores an audit event with display name, agent identity, timestamp, and event type

#### Scenario: Lease lifecycle is audited
- **WHEN** a lease starts or ends
- **THEN** the server stores an audit event with display name, lease id, timestamp, source address when available, and release reason when applicable

#### Scenario: Password failure is audited
- **WHEN** a password validation fails
- **THEN** the server stores an audit event without storing the submitted password

### Requirement: Server does not persist sensitive relay data
The server SHALL NOT persist device passwords or ADB payload bytes.

#### Scenario: Password is forwarded
- **WHEN** the server forwards a password validation request to a device client
- **THEN** the submitted password is not written to logs or persistent storage

#### Scenario: ADB bytes are relayed
- **WHEN** the server relays ADB payload bytes
- **THEN** the server records only metadata such as byte counts and does not write payload bytes to disk

### Requirement: Server exposes operational console status
The server console SHALL expose commands for inspecting devices, active leases, and audit events.

#### Scenario: Inspect devices
- **WHEN** an operator runs `adb-remote-server devices`
- **THEN** the command shows current online device display names, availability, password requirement, and lease state

#### Scenario: Inspect leases
- **WHEN** an operator runs `adb-remote-server leases`
- **THEN** the command shows active lease ids, display names, source information when available, and lease start times

#### Scenario: Inspect audit events
- **WHEN** an operator runs `adb-remote-server audit`
- **THEN** the command shows recent persisted audit events

### Requirement: User and device clients expose local status
The console clients SHALL expose status output useful for diagnosing connection and relay issues.

#### Scenario: Device client status
- **WHEN** an operator runs `adb-remote-client status`
- **THEN** the command shows server connection state, configured devices, local ADB availability, and registration status

#### Scenario: User client leases
- **WHEN** a user runs `adb-remote leases`
- **THEN** the command shows leases owned by the current user client session when available
