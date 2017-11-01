use std::str::FromStr;

use hyper::{Uri, StatusCode};
use hyper::header::ContentType;
use {Middleware, Next, Request, Response, WebFuture, HttpError};

#[macro_export]
macro_rules! combine {
    ( $( $x:expr ),* ) => {
        {
            // TODO: share CPU Pool?
            let mut app = App::new();
            $(
                app.add($x);
            )*
            app.build()
        }
    };
}

pub struct MountMiddleware<M> {
    path: String,
    middleware: M,
}

impl<S, M> Middleware<S> for MountMiddleware<M>
where
    S: 'static,
    M: Middleware<S>,
{
    fn handle(&self, mut req: Request, res: Response, ctx: S, next: Next<S>) -> WebFuture {
        if req.uri().path().starts_with(self.path.as_str()) {
            let uri_before = req.uri().clone();

            // TODO: extend hyper to not have to create a URI from string
            let new_uri = {
                let mut s = String::from("");
                if let Some(scheme) = uri_before.scheme() {
                    s += scheme;
                    s += "://";
                }
                if let Some(authority) = uri_before.authority() {
                    s += authority;
                }

                let (_, mut new_path) = uri_before.path().split_at(self.path.len());
                if new_path.len() == 0 {
                    new_path = "/";
                }
                s += new_path;

                if let Some(query) = uri_before.query() {
                    s += "?";
                    s += query;
                }
                // TODO: does not contain fragment (because currently not exposed by hyper)
                Uri::from_str(s.as_str()).unwrap()
            };

            req.set_uri(new_uri);

            Box::new(self.middleware.handle(
                req,
                res,
                ctx,
                Next::new(|mut req: Request, res, ctx| {
                    req.set_uri(uri_before);
                    next(req, res, ctx)
                }),
            ))
        } else {
            next(req, res, ctx)
        }
    }
}

pub fn mount<S, M: Middleware<S>>(path: &str, mw: M) -> MountMiddleware<M> {
    MountMiddleware {
        path: path.to_owned(),
        middleware: mw,
    }
}


#[cfg(feature = "json")]
impl From<::serde_json::Error> for HttpError {
    fn from(err: ::serde_json::Error) -> Self {
        eprintln!("Error converting to json: {}", err);
        HttpError::Status(StatusCode::BadRequest)
    }
}

#[cfg(feature = "json")]
pub fn json_response<T>(res: Response, data: T) -> Result<Response, HttpError>
where
    T: ::serde::Serialize,
{
    use serde_json as json;

    let body = json::to_string(&data)?;
    Ok(res.with_header(ContentType::json()).with_body(body))
}

#[cfg(test)]
mod tests {
    use ctx::{background, Context};
    use {default_fallback, mount, App};
    use hyper::{Request, Response};
    use hyper::{Method, Uri};
    use std::str::FromStr;
    use futures::Future;
    use std::sync::{Arc, Mutex};

    #[test]
    fn combine() {
        let mut app = App::<Context>::new();
        app.add(combine!(|_, res, _, _| Ok(res), |_, res, _, _| Ok(res)));
    }

    #[test]
    fn mount_middleware_called() {
        let called = Arc::new(Mutex::new(false));

        let app = {
            let mut app = App::new();
            let called = called.clone();
            app.add(mount("/foo", move |_, res, _, _| {
                *called.lock().unwrap() = true;
                Ok(res)
            }));
            app
        };

        let req = Request::new(Method::Get, Uri::from_str("http://localhost/foo").unwrap());
        let res = Response::default();
        app.build()
            .execute(req, res, background(), default_fallback)
            .wait()
            .unwrap();

        assert_eq!(*called.lock().unwrap(), true);
    }

    #[test]
    fn mount_middleware_not_called() {
        let called = Arc::new(Mutex::new(false));

        let app = {
            let mut app = App::new();
            let called = called.clone();
            app.add(mount("/foo", move |_, res, _, _| {
                *called.lock().unwrap() = true;
                Ok(res)
            }));
            app
        };

        let req = Request::new(Method::Get, Uri::from_str("http://localhost/bar").unwrap());
        let res = Response::default();
        app.build()
            .execute(req, res, background(), default_fallback)
            .wait()
            .unwrap();

        assert_eq!(*called.lock().unwrap(), false);
    }
}
