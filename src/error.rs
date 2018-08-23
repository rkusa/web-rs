use std::sync::Mutex;

pub use hyper::StatusCode;
use hyper::{Body, Response};
use {IntoHttpResponse, ResponseResult};

#[derive(Debug, Fail)]
pub enum HttpError {
    #[fail(display = "HTTP Error with Status {}", _0)]
    Status(StatusCode),
    #[fail(display = "HTTP Error with Status {}: {}", _0, _1)]
    StatusAndReason(StatusCode, String),
    #[fail(display = "HTTP Error with Response")]
    Response(Mutex<Response<Body>>),
}

impl IntoHttpResponse for HttpError {
    fn into_http_response(self) -> ResponseResult {
        match self {
            HttpError::Status(status) => {
                let mut res = Response::builder();
                res.status(status);
                let res = if let Some(reason) = status.canonical_reason() {
                    res.body(reason.into())?
                } else {
                    res.body(Body::empty())?
                };
                Ok(res)
            }
            HttpError::StatusAndReason(status, reason) => {
                let mut res = Response::builder();
                res.status(status);
                let res = res.body(reason.into())?;
                Ok(res)
            }
            HttpError::Response(resp) => Ok(resp.into_inner().unwrap()),
        }
    }
}

impl From<StatusCode> for HttpError {
    fn from(status: StatusCode) -> Self {
        HttpError::Status(status)
    }
}

impl From<Response<Body>> for HttpError {
    fn from(res: Response<Body>) -> Self {
        HttpError::Response(Mutex::new(res))
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
