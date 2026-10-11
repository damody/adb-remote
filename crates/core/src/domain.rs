use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    NameAlreadyExists,
    DeviceOffline,
    DeviceBusy,
    PasswordRequired,
    InvalidPassword,
    AdbUnavailable,
    LeaseExpired,
    RelayFailed,
    RateLimited,
    NotFound,
    InvalidArgument,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NameAlreadyExists => "NameAlreadyExists",
            Self::DeviceOffline => "DeviceOffline",
            Self::DeviceBusy => "DeviceBusy",
            Self::PasswordRequired => "PasswordRequired",
            Self::InvalidPassword => "InvalidPassword",
            Self::AdbUnavailable => "AdbUnavailable",
            Self::LeaseExpired => "LeaseExpired",
            Self::RelayFailed => "RelayFailed",
            Self::RateLimited => "RateLimited",
            Self::NotFound => "NotFound",
            Self::InvalidArgument => "InvalidArgument",
            Self::Internal => "Internal",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<ErrorCode> for tonic::Status {
    fn from(value: ErrorCode) -> Self {
        let code = match value {
            ErrorCode::InvalidArgument
            | ErrorCode::PasswordRequired
            | ErrorCode::InvalidPassword => tonic::Code::InvalidArgument,
            ErrorCode::NotFound | ErrorCode::DeviceOffline => tonic::Code::NotFound,
            ErrorCode::DeviceBusy | ErrorCode::NameAlreadyExists | ErrorCode::RateLimited => {
                tonic::Code::FailedPrecondition
            }
            ErrorCode::LeaseExpired => tonic::Code::Unauthenticated,
            ErrorCode::AdbUnavailable | ErrorCode::RelayFailed | ErrorCode::Internal => {
                tonic::Code::Internal
            }
        };
        tonic::Status::new(code, value.as_str())
    }
}

#[derive(Debug, Error)]
pub enum AdbRemoteError {
    #[error("{0}")]
    Code(ErrorCode),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    Available,
    Leased,
    Offline,
    Unavailable,
}

impl DeviceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Leased => "leased",
            Self::Offline => "offline",
            Self::Unavailable => "unavailable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(ErrorCode::DeviceBusy.as_str(), "DeviceBusy");
        assert_eq!(ErrorCode::InvalidPassword.as_str(), "InvalidPassword");
    }

    #[test]
    fn tonic_mapping_keeps_business_errors_readable() {
        let status: tonic::Status = ErrorCode::DeviceBusy.into();
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert_eq!(status.message(), "DeviceBusy");
    }
}
