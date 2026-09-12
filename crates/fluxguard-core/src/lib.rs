//! Provider-neutral FluxGuard domain model.

mod budget;
mod error;
mod operation;
mod policy;
mod pressure;
mod source;

pub use budget::{
    Applicability, Availability, BlockReason, BudgetSnapshot, BudgetWindow, DecimalValue,
    Freshness, MetricDimension, SnapshotWarning, WindowId,
};
pub use error::DomainError;
pub use operation::{
    estimate_operation_cost, OperationCost, OperationImportance, OperationKind, OperationProfile,
};
pub use policy::{
    advise, advise_with_profile, ExecutionAdvice, ExecutionMode, PolicyConfig, PolicyProfile,
    ProceedDecision, Recommendation,
};
pub use pressure::{
    assess_combined, assess_combined_at, assess_snapshot, assess_snapshot_at, CombinedSnapshot,
    Confidence, ConstraintRef, PressureAssessment, PressureConfig, PressureLevel, ReasonCode,
};
pub use source::{
    Provenance, SourceCapabilities, SourceDescriptor, SourceId, SourceKind, SourceQuality,
};
