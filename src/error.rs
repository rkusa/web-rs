use std::error::Error as StdError;
use std::fmt;

use hyper::server::Response;
pub use hyper::StatusCode;

#[derive(Debug)]
pub enum Error {
    Status(StatusCode),
    Response(Response),
}

impl Error {
    pub fn into_response(self) -> Response {
        match self {
            Error::Status(status) => {
                let mut res = Response::new().with_status(status);
                if let Some(reason) = status.canonical_reason() {
                    res.set_body(reason);
                }
                res
            }
            Error::Response(resp) => resp,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Status(ref status) => write!(f, "Error with status: {}", status),
            Error::Response(ref resp) => write!(f, "Error with response:\n{:?}", resp),
        }
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Status(_) => "Error with status code",
            Error::Response(_) => "Error with response",
        }
    }
}

#[macro_export]
macro_rules! ok {
    ($cond:expr) => (
        if !$cond {
            return $crate::Respond::Error(
                $crate::Error::Status($crate::error::StatusCode::BadRequest)
            );
        }
    );
    ($cond:expr, $status:expr) => (
        if !$cond {
            return $crate::Respond::Error($crate::Error::Status($status));
        }
    );
    ($cond:expr, $status:expr, $($arg:tt)+) => (
        if !$cond {
            return $crate::Respond::Error(
                $crate::Error::Response($crate::Response::new()
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
            None => return $crate::Respond::Error(
                $crate::Error::Status($crate::error::StatusCode::NotFound)
            )
        }
    );
    ($option:expr, $status:expr) => (
        match $option {
            Some(val) => val,
            None => return $crate::Respond::Error(
                $crate::Error::Status($status)
            )
        }
    );
    ($option:expr, $status:expr, $($arg:tt)+) => (
        match $option {
            Some(val) => val,
            None => return $crate::Respond::Error(
                $crate::Error::Response($crate::Response::new()
                    .with_status($status)
                    .with_body(format!($($arg)+)))
            )
        }
    );
}
