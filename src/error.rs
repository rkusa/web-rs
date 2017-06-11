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
            Error::Status(status) => {
                Response::new().with_status(status)
            }
            Error::Response(resp) => resp,
        }
    }
}