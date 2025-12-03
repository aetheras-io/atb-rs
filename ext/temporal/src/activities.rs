use prost_wkt_types::Duration;
use temporalio_common::protos::{
    coresdk::{
        FromJsonPayloadExt, PayloadDeserializeErr,
        activity_result::{ActivityResolution, Cancellation, Failure, activity_resolution::Status},
    },
    temporal::api::{
        common::v1::{Payload, Payloads},
        enums::v1::TimeoutType,
        failure::v1::{
            ActivityFailureInfo, ApplicationFailureInfo, CanceledFailureInfo,
            ChildWorkflowExecutionFailureInfo, Failure as ApiFailure, NexusHandlerFailureInfo,
            NexusOperationFailureInfo, ResetWorkflowFailureInfo, ServerFailureInfo,
            TerminatedFailureInfo, TimeoutFailureInfo, failure::FailureInfo,
        },
    },
};

pub type ActivityResult<T> = Result<T, ActivityRunError>;

#[derive(Debug, thiserror::Error)]
pub enum ActivityRunError {
    #[error("activity completed without a status")]
    MissingStatus,

    #[error("activity completed successfully but did not return a payload")]
    MissingPayload,

    #[error(transparent)]
    Failure(#[from] FailureError),

    #[error("activity requested backoff: attempt {attempt}, backoff {backoff_duration:?}")]
    Backoff {
        attempt: u32,
        backoff_duration: Option<Duration>,
    },

    #[error("activity payload deserialization failed: {0}")]
    PayloadDecode(#[from] PayloadDeserializeErr),

    #[error("activity payload serialization failed: {0}")]
    PayloadEncode(anyhow::Error),

    #[error("malformed activity resolution: {0}")]
    Malformed(&'static str),
}

#[derive(Debug, thiserror::Error)]
pub enum FailureError {
    #[error("application failure: {message}")]
    Application {
        message: String,
        non_retryable: bool,
    },

    #[error("timeout failure: {message}")]
    Timeout {
        message: String,
        /// Raw enum value from the proto; use `TimeoutType::from_i32` to interpret.
        timeout_type: i32,
    },

    #[error("canceled: {message}")]
    Canceled {
        message: String,
        details: Option<Payloads>,
    },

    #[error("terminated: {message}")]
    Terminated { message: String },

    #[error("server failure: {message}")]
    Server { message: String },

    #[error("reset workflow failure: {message}")]
    ResetWorkflow { message: String },

    #[error("activity failure: {message}")]
    Activity { message: String },

    #[error("child workflow failure: {message}")]
    ChildWorkflow { message: String },

    #[error("nexus operation failure: {message}")]
    NexusOperation { message: String },

    #[error("nexus handler failure: {message}")]
    NexusHandler { message: String },

    #[error("unknown failure kind: {message}")]
    Unknown { message: String },
}

impl FailureError {
    pub fn from_api(f: ApiFailure) -> Self {
        let leaf = Self::root_cause(&f);

        match leaf.failure_info.as_ref() {
            Some(FailureInfo::ApplicationFailureInfo(ApplicationFailureInfo {
                non_retryable,
                ..
            })) => FailureError::Application {
                message: leaf.message.clone(),
                non_retryable: *non_retryable,
            },

            Some(FailureInfo::TimeoutFailureInfo(TimeoutFailureInfo { timeout_type, .. })) => {
                FailureError::Timeout {
                    message: leaf.message.clone(),
                    timeout_type: *timeout_type,
                }
            }

            Some(FailureInfo::CanceledFailureInfo(CanceledFailureInfo { details, .. })) => {
                FailureError::Canceled {
                    message: leaf.message.clone(),
                    details: details.clone(),
                }
            }

            Some(FailureInfo::TerminatedFailureInfo(TerminatedFailureInfo { .. })) => {
                FailureError::Terminated {
                    message: leaf.message.clone(),
                }
            }

            Some(FailureInfo::ServerFailureInfo(ServerFailureInfo { .. })) => {
                FailureError::Server {
                    message: leaf.message.clone(),
                }
            }

            Some(FailureInfo::ResetWorkflowFailureInfo(ResetWorkflowFailureInfo { .. })) => {
                FailureError::ResetWorkflow {
                    message: leaf.message.clone(),
                }
            }

            Some(FailureInfo::ActivityFailureInfo(ActivityFailureInfo { .. })) => {
                FailureError::Activity {
                    message: leaf.message.clone(),
                }
            }

            Some(FailureInfo::ChildWorkflowExecutionFailureInfo(
                ChildWorkflowExecutionFailureInfo { .. },
            )) => FailureError::ChildWorkflow {
                message: leaf.message.clone(),
            },

            Some(FailureInfo::NexusOperationExecutionFailureInfo(NexusOperationFailureInfo {
                ..
            })) => FailureError::NexusOperation {
                message: leaf.message.clone(),
            },

            Some(FailureInfo::NexusHandlerFailureInfo(NexusHandlerFailureInfo { .. })) => {
                FailureError::NexusHandler {
                    message: leaf.message.clone(),
                }
            }

            None => FailureError::Unknown {
                message: leaf.message.clone(),
            },
        }
    }

    fn root_cause(mut failure: &ApiFailure) -> &ApiFailure {
        while let Some(ref cause) = failure.cause {
            failure = cause;
        }
        failure
    }
}

/// Convert an ActivityResolution into an optional Payload, or an ActivityRunError.
///
/// - Completed → Ok(Some(payload)) or Ok(None)
/// - Failed    → Err(ActivityRunError::Failure(_))
/// - Cancelled → Err(ActivityRunError::Failure(_))
/// - Backoff   → Err(ActivityRunError::Backoff { .. })
pub fn resolution_payload(res: ActivityResolution) -> Result<Option<Payload>, ActivityRunError> {
    let status = res.status.ok_or(ActivityRunError::MissingStatus)?;

    match status {
        Status::Completed(success) => Ok(success.result),

        Status::Failed(Failure { failure }) => {
            let failure = failure.ok_or(ActivityRunError::Malformed(
                "Failure.status=Failed but missing Failure message",
            ))?;
            Err(FailureError::from_api(failure).into())
        }

        Status::Cancelled(Cancellation { failure }) => {
            let failure = failure.ok_or(ActivityRunError::Malformed(
                "Failure.status=Cancelled but missing Failure message",
            ))?;
            Err(FailureError::from_api(failure).into())
        }

        Status::Backoff(bo) => Err(ActivityRunError::Backoff {
            attempt: bo.attempt,
            backoff_duration: bo.backoff_duration,
        }),
    }
}

/// Convert an ActivityResolution into a typed value `T` using `FromJsonPayloadExt`.
///
/// - Success + payload present → deserialize into `T`
/// - Success + no payload      → Err(MissingPayload)
/// - Failure / Cancel / Backoff → mapped to ActivityRunError variants
pub fn resolution_value<T>(res: ActivityResolution) -> ActivityResult<T>
where
    T: FromJsonPayloadExt,
{
    let maybe_payload = resolution_payload(res)?;
    let payload = maybe_payload.ok_or(ActivityRunError::MissingPayload)?;
    let value = T::from_json_payload(&payload)?;
    Ok(value)
}

/// For activities whose return type is `()`.
pub fn resolution_unit(res: ActivityResolution) -> ActivityResult<()> {
    // Ensure it was successful; ignore payload if present.
    let _ = resolution_payload(res)?;
    Ok(())
}

impl ActivityRunError {
    /// If this error represents a timeout failure, return the concrete `TimeoutType`.
    pub fn timeout_info(&self) -> Option<(&String, TimeoutType)> {
        match self {
            ActivityRunError::Failure(FailureError::Timeout {
                timeout_type,
                message,
            }) => Some((
                message,
                TimeoutType::try_from(*timeout_type)
                    .expect("temporal timeout enum maps correctly. qed"),
            )),
            _ => None,
        }
    }

    /// If this error represents a cancellation, return the cancellation details payloads, if any.
    pub fn cancellation_details(&self) -> Option<(&String, &Option<Payloads>)> {
        match self {
            ActivityRunError::Failure(FailureError::Canceled { details, message }) => {
                Some((message, details))
            }
            _ => None,
        }
    }
}
