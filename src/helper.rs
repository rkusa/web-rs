use std::str::FromStr;

use futures::Future;
use hyper::Uri;
use {Middleware, next, Request, Response, Context, WebFuture};
use Respond::*;

#[macro_export]
macro_rules! combine {
    ( $( $x:expr ),* ) => {
        {
            // TODO: share CPU Pool?
            let mut app = App::new(|| $crate::ctx::background());
            $(
                app.add($x);
            )*
            app
        }
    };
}

pub struct MountMiddleware<M: Middleware> {
    path: String,
    middleware: M,
}

impl<M> Middleware for MountMiddleware<M>
where
    M: Middleware,
{
    fn handle(&self, mut req: Request, res: Response, ctx: Context) -> WebFuture {
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

            Box::new(self.middleware.handle(req, res, ctx).map(|r| match r {
                Next(mut req, res, ctx) => {
                    req.set_uri(uri_before);
                    Next(req, res, ctx)
                }
                _ => r,
            }))
        } else {
            next(req, res, ctx)
        }
    }

    fn after(&self, res: &Response) {
        // TODO: ONLY WHEN ACTUALLY CALLED
        self.middleware.after(res)
    }
}

pub fn mount<M: Middleware>(path: &str, mw: M) -> MountMiddleware<M> {
    MountMiddleware {
        path: path.to_owned(),
        middleware: mw,
    }
}

#[cfg(test)]
mod tests {
    use ctx::{background, Context};
    use {App, next, WebFuture, Middleware, mount};
    use hyper::{Request, Response};
    use hyper::{Method, Uri};
    use std::str::FromStr;
    use futures::Future;
    use std::sync::{Arc, Mutex};

    #[test]
    fn combine() {
        let mut app = App::new(|| background());
        app.add(combine!(
            |req, res, ctx| Ok((req, res, ctx)),
            |req, res, ctx| Ok((req, res, ctx))
        ));
    }

    #[test]
    fn mount_middleware() {
        struct TestMiddleware {
            called: Arc<Mutex<bool>>,
        }

        impl Middleware for TestMiddleware {
            fn handle(&self, req: Request, res: Response, ctx: Context) -> WebFuture {
                *self.called.lock().unwrap() = true;
                next(req, res, ctx)
            }
        }

        let called = Arc::new(Mutex::new(false));

        let mut app = App::new(|| background());
        app.add(mount("/test", TestMiddleware { called: called.clone() }));

        let req = Request::new(Method::Get, Uri::from_str("http://localhost/test").unwrap());
        let res = Response::default();
        app.execute(req, res, background()).wait().unwrap();

        assert_eq!(*called.lock().unwrap(), true);
    }

    #[test]
    fn mount_middleware_after() {
        struct TestMiddleware {
            called: Arc<Mutex<bool>>,
        }

        impl Middleware for TestMiddleware {
            fn handle(&self, req: Request, res: Response, ctx: Context) -> WebFuture {
                next(req, res, ctx)
            }

            fn after(&self, _res: &Response) {
                *self.called.lock().unwrap() = true;
            }
        }

        let first = Arc::new(Mutex::new(false));
        let second = Arc::new(Mutex::new(false));

        let mut app = App::new(|| background());
        app.add(mount("/foo", TestMiddleware { called: first.clone() }));
        app.add(mount("/bar", TestMiddleware { called: second.clone() }));

        let req = Request::new(Method::Get, Uri::from_str("http://localhost/bar").unwrap());
        let res = Response::default();
        app.execute(req, res, background()).wait().unwrap();

        assert_eq!(*first.lock().unwrap(), false);
        assert_eq!(*second.lock().unwrap(), true);
    }
}
