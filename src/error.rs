use std::error::Error as StdError;
use std::fmt;

use http;
pub use hyper::StatusCode;
use hyper::{Body, Response};

#[derive(Debug)]
pub enum HttpError {
    Status(StatusCode),
    Response(Response<Body>),
    Http(http::Error),
}

impl HttpError {
    pub fn into_response(self) -> Result<Response<Body>, http::Error> {
        match self {
            HttpError::Status(status) => {
                let mut res = Response::builder();
                res.status(status);
                if let Some(reason) = status.canonical_reason() {
                    res.body(reason.into())
                } else {
                    res.body(Body::empty())
                }
            }
            HttpError::Response(resp) => Ok(resp),
            HttpError::Http(err) => Err(err),
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HttpError::Status(ref status) => write!(f, "Error with status: {}", status),
            HttpError::Response(ref resp) => write!(f, "Error with response:\n{:?}", resp),
            HttpError::Http(ref err) => write!(f, "Error with http:\n{:?}", err),
        }
    }
}

impl StdError for HttpError {
    fn description(&self) -> &str {
        match *self {
            HttpError::Status(_) => "Error with status code",
            HttpError::Response(_) => "Error with response",
            HttpError::Http(_) => "Error with http",
        }
    }
}

impl From<StatusCode> for HttpError {
    fn from(status: StatusCode) -> Self {
        HttpError::Status(status)
    }
}

impl From<http::Error> for HttpError {
    fn from(err: http::Error) -> Self {
        HttpError::Http(err)
    }
}

#[macro_export]
macro_rules! ok {
    ($cond:expr) => (
        if !$cond {
            return Err(
                $crate::HttpError::Status($crate::error::StatusCode::BAD_REQUEST)
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
                $crate::HttpError::Status($crate::error::StatusCode::NOT_FOUND)
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
