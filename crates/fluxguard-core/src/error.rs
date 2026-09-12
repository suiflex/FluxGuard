use thiserror::Error;

/// Validation failures for normalized FluxGuard domain values.
#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("identifier must not be empty")]
    EmptyIdentifier,
    #[error("value must be finite")]
    NonFiniteValue,
    #[error("pressure thresholds must be ordered between 0 and 100")]
    InvalidThresholds,
    #[error("percentage must be between 0 and 100")]
    PercentageOutOfRange,
    #[error("used and remaining percentages must sum to 100")]
    InconsistentPercentages,
    #[error("used, limit, and remaining amounts are inconsistent")]
    InconsistentAmounts,
    #[error("timestamp must use UTC")]
    NonUtcTimestamp,
}
