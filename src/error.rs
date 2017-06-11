use std::error::Error as StdError;
use std::fmt;

use hyper::server::Response;
use hyper::status::StatusCode;

#[derive(Debug)]
pub enum Error {
    Status(StatusCode),
    Response(Response),
}

impl Error {
    pub fn into_response(self) -> Response {
        match self {
            Error::Status(status) => Response::new().with_status(status),
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
