use std::error::Error as StdError;
use std::fmt;

use hyper::server::Response;
pub use hyper::StatusCode;

#[derive(Debug)]
pub enum HttpError {
    Status(StatusCode),
    Response(Response),
}

impl HttpError {
    pub fn into_response(self) -> Response {
        match self {
            HttpError::Status(status) => {
                let mut res = Response::new().with_status(status);
                if let Some(reason) = status.canonical_reason() {
                    res.set_body(reason);
                }
                res
            }
            HttpError::Response(resp) => resp,
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HttpError::Status(ref status) => write!(f, "Error with status: {}", status),
            HttpError::Response(ref resp) => write!(f, "Error with response:\n{:?}", resp),
        }
    }
}

impl StdError for HttpError {
    fn description(&self) -> &str {
        match *self {
            HttpError::Status(_) => "Error with status code",
            HttpError::Response(_) => "Error with response",
        }
    }
}

impl From<StatusCode> for HttpError {
    fn from(status: StatusCode) -> Self {
        HttpError::Status(status)
    }
}

#[macro_export]
macro_rules! ok {
    ($cond:expr) => (
        if !$cond {
            return Err(
                $crate::HttpError::Status($crate::error::StatusCode::BadRequest)
            );
        }
    );
    ($cond:expr, $status:expr) => (
        if !$cond {
            return Err($crate::HttpError::Status($status));
        }
    );
    ($cond:expr, $status:expr, $($arg:tt)+) => (
        if !$cond {
            return Err(
                $crate::HttpError::Response($crate::Response::new()
                    .with_status($status)
                    .with_body(format!($($arg)+)))
            );
        }
    );
}

#[macro_export]
macro_rules! ok_some {
    ($option:expr) => (
        match $option {
            Some(val) => val,
            None => return Err(
                $crate::HttpError::Status($crate::error::StatusCode::NotFound)
            )
        }
    );
    ($option:expr, $status:expr) => (
        match $option {
            Some(val) => val,
            None => return Err(
                $crate::HttpError::Status($status)
            )
        }
    );
    ($option:expr, $status:expr, $($arg:tt)+) => (
        match $option {
            Some(val) => val,
            None => return Err(
                $crate::HttpError::Response($crate::Response::new()
                    .with_status($status)
                    .with_body(format!($($arg)+)))
            )
        }
    );
}
