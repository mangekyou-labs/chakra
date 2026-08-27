//! Chakra API envelope: `{success, data, error: {code, message}}`.

use serde::Serialize;

/// Machine-readable error codes (T4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiErrorCode {
    InvalidParams,
    ZeroAmount,
    SameToken,
    UnknownToken,
    NoRoute,
    RateLimited,
    RouteInvalid,
    Paused,
    NotReady,
    RpcError,
}

impl ApiErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidParams => "INVALID_PARAMS",
            Self::ZeroAmount => "ZERO_AMOUNT",
            Self::SameToken => "SAME_TOKEN",
            Self::UnknownToken => "UNKNOWN_TOKEN",
            Self::NoRoute => "NO_ROUTE",
            Self::RateLimited => "RATE_LIMITED",
            Self::RouteInvalid => "ROUTE_INVALID",
            Self::Paused => "PAUSED",
            Self::NotReady => "NOT_READY",
            Self::RpcError => "RPC_ERROR",
        }
    }
}

/// Error body inside the envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.as_str().to_string(),
            message: message.into(),
        }
    }
}

/// Standard success envelope: `{success: true, data, error: null}`.
#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
}

impl Envelope<serde_json::Value> {
    /// JSON-value envelope with an error body.
    pub fn err(code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self::err_from(ApiError::new(code, message))
    }

    /// JSON-value envelope from a prebuilt `ApiError`.
    pub fn err_from(error: ApiError) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}
