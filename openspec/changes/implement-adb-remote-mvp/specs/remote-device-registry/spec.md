## ADDED Requirements

### Requirement: Device client scans local ADB devices
The device-side client SHALL discover Android devices available through the local Windows ADB installation.

#### Scenario: Local ADB devices are listed
- **WHEN** the operator runs `adb-remote-client scan`
- **THEN** the client lists devices reported by local ADB with their serials and available details

#### Scenario: ADB is unavailable
- **WHEN** the local ADB executable or ADB server cannot be reached
- **THEN** the client reports `AdbUnavailable` and does not register devices as online

### Requirement: Device client stores local shared device configuration
The device-side client SHALL allow an operator to configure an ADB serial with a display name and either password-protected or passwordless access.

#### Scenario: Add passwordless device
- **WHEN** the operator runs `adb-remote-client add-device <adb-serial> --name <display-name> --no-password`
- **THEN** the client stores the device mapping locally with `password_required` set to false

#### Scenario: Add password-protected device
- **WHEN** the operator runs `adb-remote-client add-device <adb-serial> --name <display-name> --password`
- **THEN** the client prompts interactively for the password and stores it only in local device-client configuration

### Requirement: Device client registers online configured devices
The device-side client SHALL register configured devices with the server when the configured ADB serial is currently available.

#### Scenario: Configured serial is online
- **WHEN** `adb-remote-client serve` connects to the server and a configured serial is present locally
- **THEN** the server records the device as online with its display name, ADB serial, agent session, and password requirement

#### Scenario: Configured serial is missing
- **WHEN** `adb-remote-client serve` connects to the server and a configured serial is not present locally
- **THEN** the client reports the device as unavailable and the server does not list it as online

### Requirement: Active display names are unique
The server SHALL reject registrations whose display name conflicts with another active online registration.

#### Scenario: Display name is available
- **WHEN** a device client registers a display name that has no active conflict
- **THEN** the server accepts the registration and includes the device in list results

#### Scenario: Display name conflicts
- **WHEN** a device client registers a display name that is already active
- **THEN** the server rejects the registration with `NameAlreadyExists`

### Requirement: Device online state follows agent connectivity
The server SHALL mark devices offline when their owning device client session disconnects or misses heartbeat timeout.

#### Scenario: Device agent disconnects
- **WHEN** an active device client session disconnects
- **THEN** all devices owned by that session become offline and disappear from online list results

#### Scenario: Device agent reconnects
- **WHEN** the device client reconnects and re-registers available configured devices
- **THEN** the server lists those devices online again if their display names do not conflict
