extern crate ctx;
extern crate futures;
extern crate futures_cpupool;
extern crate hyper;

pub mod error;
#[macro_use]
mod helper;
pub use helper::*;

use std::sync::Arc;

pub use ctx::Context;
use futures_cpupool::CpuPool;
pub use hyper::{Request, Response, Body};
use hyper::server::Service;
use hyper::StatusCode;
pub use error::HttpError;
use std::ops::Deref;
use futures::{future, Future, Poll, Async, IntoFuture};

pub enum Respond {
    Next(Request, Response, Context),
    Done(Response),
}

pub use Respond::*;

impl From<(Request, Response, Context)> for Respond {
    fn from(args: (Request, Response, Context)) -> Self {
        Respond::Next(args.0, args.1, args.2)
    }
}

impl From<Response> for Respond {
    fn from(res: Response) -> Self {
        Respond::Done(res)
    }
}

impl<B> From<(Response, B)> for Respond
where
    B: Into<Body>,
{
    fn from(args: (Response, B)) -> Self {
        let (mut res, body) = args;
        res.set_body(body);
        Respond::Done(res)
    }
}

pub type WebFuture = Box<Future<Item = Respond, Error = HttpError> + Send>;

pub fn next(req: Request, res: Response, ctx: Context) -> WebFuture {
    Box::new(future::ok(Next(req, res, ctx)))
}

pub fn done(res: Response) -> WebFuture {
    Box::new(future::ok(Done(res)))
}

pub trait Middleware: Send + Sync {
    fn handle(&self, Request, Response, Context) -> WebFuture;
    fn after(&self, _res: &Response) {}
}

impl Middleware for Box<Middleware> {
    fn handle(&self, req: Request, res: Response, ctx: Context) -> WebFuture {
        self.deref().handle(req, res, ctx)
    }

    fn after(&self, res: &Response) {
        self.deref().after(res)
    }
}

pub trait IntoWebFuture {
    fn into_future(self) -> WebFuture;
}

impl<F, I> IntoWebFuture for F
where
    F: IntoFuture<Item = I, Error = HttpError>,
    I: Into<Respond>,
    <F as futures::IntoFuture>::Future: std::marker::Send + 'static,
{
    fn into_future(self) -> WebFuture {
        Box::new(self.into_future().map(|i| i.into()))
    }
}

impl<F, B> Middleware for F
where
    F: Fn(Request, Response, Context) -> B + Send + Sync,
    B: IntoWebFuture + 'static,
{
    fn handle(&self, req: Request, res: Response, ctx: Context) -> WebFuture {
        Box::new((self)(req, res, ctx).into_future())
    }
}

pub struct AppBuilder<F>
where
    F: Fn() -> Context + Send,
{
    middlewares: Vec<Box<Middleware>>,
    pool: CpuPool,
    context_factory: Arc<F>,
}

impl<F> AppBuilder<F>
where
    F: Fn() -> Context + Send + Sync,
{
    pub fn new(ctx: F) -> Self {
        AppBuilder {
            middlewares: Vec::new(),
            pool: CpuPool::new(32),
            context_factory: Arc::new(ctx),
        }
    }

    pub fn add<M>(&mut self, middleware: M)
    where
        M: Middleware + 'static,
    {
        self.middlewares.push(Box::new(middleware));
    }

    // TODO: better name
    pub fn add_sync<M>(&mut self, middleware: M)
    where
        M: Middleware + 'static,
    {
        let pool = self.pool.clone();
        self.add(SyncMiddleware {
            pool: pool,
            mw: Arc::new(middleware),
        });
    }

    // TODO: better name
    pub fn offload<M>(&self, middleware: M) -> SyncMiddleware<M>
    where
        M: Middleware + 'static,
    {
        SyncMiddleware {
            pool: self.pool.clone(),
            mw: Arc::new(middleware),
        }
    }

    pub fn build(self) -> App<F> {
        App {
            middlewares: Arc::new(self.middlewares),
            pool: self.pool,
            context_factory: self.context_factory,
        }
    }
}

pub struct App<F>
where
    F: Fn() -> Context + Send,
{
    middlewares: Arc<Vec<Box<Middleware>>>,
    pool: CpuPool,
    context_factory: Arc<F>,
}

impl<F> Clone for App<F>
where
    F: Fn() -> Context + Send,
{
    fn clone(&self) -> Self {
        App {
            middlewares: self.middlewares.clone(),
            pool: self.pool.clone(),
            context_factory: self.context_factory.clone(),
        }
    }
}

impl<F> App<F>
where
    F: Fn() -> Context + Send + Sync,
{
    pub fn new(ctx: F) -> AppBuilder<F> {
        AppBuilder::new(ctx)
    }

    fn execute(&self, req: Request, res: Response, ctx: Context) -> Execution {
        Execution {
            args: Some((req, res, ctx)),
            pos: 0,
            middlewares: self.middlewares.clone(),
            curr: None,
        }
    }
}

impl<F> Middleware for App<F>
where
    F: Fn() -> Context + Send + Sync,
{
    fn handle(&self, req: Request, res: Response, ctx: Context) -> WebFuture {
        Box::new(self.execute(req, res, ctx))
    }
}

pub struct SyncMiddleware<M: Middleware> {
    pool: CpuPool,
    mw: Arc<M>,
}

impl<M> Middleware for SyncMiddleware<M>
where
    M: Middleware + 'static,
{
    fn handle(&self, req: Request, res: Response, ctx: Context) -> WebFuture {
        let mw = self.mw.clone();
        Box::new(self.pool.spawn_fn(move || mw.handle(req, res, ctx)))
    }

    fn after(&self, res: &Response) {
        self.mw.after(res)
    }
}

struct Execution {
    args: Option<(Request, Response, Context)>,
    pos: usize,
    middlewares: Arc<Vec<Box<Middleware>>>,
    curr: Option<WebFuture>,
}

impl Execution {
    fn after(&self, res: &Response) {
        for i in (0..self.pos).rev() {
            if let Some(mw) = self.middlewares.get(i) {
                mw.after(&res);
            }
        }
    }
}

impl Future for Execution {
    type Item = Respond;
    type Error = HttpError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(mut curr) = self.curr.take() {
            match curr.poll() {
                Ok(Async::Ready(r)) => {
                    match r {
                        Next(req, res, ctx) => {
                            self.args = Some((req, res, ctx));
                            self.poll()
                        }
                        Done(res) => {
                            self.after(&res);
                            Ok(Async::Ready(Done(res)))
                        }
                    }
                }
                Ok(Async::NotReady) => {
                    self.curr = Some(curr);
                    Ok(Async::NotReady)
                }
                Err(err) => {
                    // TODO: after?
                    Err(err)
                }
            }
        } else {
            self.curr = {
                let (req, res, ctx) = self.args.take().unwrap();
                if let Some(mw) = self.middlewares.get(self.pos) {
                    self.pos += 1;
                    Some(mw.handle(req, res, ctx))
                } else {
                    self.after(&res);
                    return Ok(Async::Ready(Next(req, res, ctx)));
                }
            };
            self.poll()
        }
    }
}

impl<F> Service for App<F>
where
    F: Fn() -> Context + Send + Sync,
{
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let ctx = (self.context_factory)();
        let resp = self.execute(req, Response::default(), ctx)
            .map(|r| match r {
                Done(res) => res,
                Next(_, _, _) => HttpError::Status(StatusCode::NotFound).into_response(),
            })
            .or_else(|err| future::ok(err.into_response()));
        Box::new(resp)
    }
}

#[cfg(test)]
mod tests {
    use ctx::{Context, background};
    use {App, next, done, WebFuture, Middleware, mount};
    use hyper::{Request, Response};
    use hyper::{Method, Uri};
    use std::str::FromStr;
    use futures::Future;
    use std::sync::{Arc, Mutex};

    #[test]
    fn closure_middleware() {
        let mut app = App::new(|| background());
        app.add(|req, mut res: Response, ctx| {
            res.set_body("Hello World!");
            next(req, res, ctx)
        });
    }

    #[test]
    fn middleware() {
        struct TestMiddleware;

        impl Middleware for TestMiddleware {
            fn handle(&self, _req: Request, res: Response, _ctx: Context) -> WebFuture {
                done(res)
            }
        }

        let mut app = App::new(|| background());
        app.add(TestMiddleware {});
    }

    #[test]
    fn fn_middleware() {
        fn handle(req: Request, mut res: Response, ctx: Context) -> WebFuture {
            res.set_body("Hello World!");
            next(req, res, ctx)
        }

        let mut app = App::new(|| background());
        app.add(handle);
    }

    #[test]
    fn end_with_done() {
        let mut app = App::new(|| background());
        app.add(|_, res, _| Ok(res));
        app.add(|_, res, _| Ok((res, "Hello World")));
    }

    #[test]
    fn end_with_next() {
        let mut app = App::new(|| background());
        app.add(|req, res, ctx| Ok((req, res, ctx)));
    }

    #[test]
    fn chain_middleware() {
        let mut app1 = App::new(|| background());
        let app2 = App::new(|| background());
        app1.add(app2.build());
    }

    #[test]
    fn after_done() {
        struct TestMiddleware {
            called: Arc<Mutex<bool>>,
        }

        impl Middleware for TestMiddleware {
            fn handle(&self, _req: Request, res: Response, _ctx: Context) -> WebFuture {
                done(res)
            }

            fn after(&self, _res: &Response) {
                *self.called.lock().unwrap() = true;
            }
        }

        let called = Arc::new(Mutex::new(false));

        let mut app = App::new(|| background());
        app.add(TestMiddleware { called: called.clone() });

        let req = Request::new(Method::Get, Uri::from_str("http://localhost").unwrap());
        let res = Response::default();
        app.build().execute(req, res, background()).wait().unwrap();

        assert_eq!(*called.lock().unwrap(), true);
    }

    #[test]
    fn after_next() {
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

        let called = Arc::new(Mutex::new(false));

        let mut app = App::new(|| background());
        app.add(TestMiddleware { called: called.clone() });

        let req = Request::new(Method::Get, Uri::from_str("http://localhost").unwrap());
        let res = Response::default();
        app.build().execute(req, res, background()).wait().unwrap();

        assert_eq!(*called.lock().unwrap(), true);
    }

    #[test]
    fn after_until_done() {
        struct ContinueMiddleware {
            after_called: Arc<Mutex<bool>>,
        }

        impl Middleware for ContinueMiddleware {
            fn handle(&self, req: Request, res: Response, ctx: Context) -> WebFuture {
                next(req, res, ctx)
            }

            fn after(&self, _res: &Response) {
                *self.after_called.lock().unwrap() = true;
            }
        }

        struct DoneMiddleware {
            after_called: Arc<Mutex<bool>>,
        }

        impl Middleware for DoneMiddleware {
            fn handle(&self, _req: Request, res: Response, _ctx: Context) -> WebFuture {
                done(res)
            }

            fn after(&self, _res: &Response) {
                *self.after_called.lock().unwrap() = true;
            }
        }

        let first = Arc::new(Mutex::new(false));
        let second = Arc::new(Mutex::new(false));
        let third = Arc::new(Mutex::new(false));
        let fourth = Arc::new(Mutex::new(false));

        let mut app = App::new(|| background());
        app.add(ContinueMiddleware { after_called: first.clone() });
        app.add(ContinueMiddleware { after_called: second.clone() });
        app.add(DoneMiddleware { after_called: third.clone() });
        app.add(DoneMiddleware { after_called: fourth.clone() });

        let req = Request::new(Method::Get, Uri::from_str("http://localhost").unwrap());
        let res = Response::default();
        app.build().execute(req, res, background()).wait().unwrap();

        assert_eq!(*first.lock().unwrap(), true);
        assert_eq!(*second.lock().unwrap(), true);
        assert_eq!(*third.lock().unwrap(), true);
        assert_eq!(*fourth.lock().unwrap(), false);
    }

    #[test]
    fn after_until_done_nested() {
        struct TestMiddleware {
            id: usize,
            call_order: Arc<Mutex<Vec<usize>>>,
        }

        impl Middleware for TestMiddleware {
            fn handle(&self, req: Request, res: Response, ctx: Context) -> WebFuture {
                if self.id == 4 {
                    done(res)
                } else {
                    next(req, res, ctx)
                }
            }

            fn after(&self, _res: &Response) {
                self.call_order.lock().unwrap().push(self.id);
            }
        }

        let call_order = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new(|| background());
        app.add(TestMiddleware {
            id: 1,
            call_order: call_order.clone(),
        });
        app.add(mount(
            "/foo",
            combine!(
                TestMiddleware {
                    id: 2,
                    call_order: call_order.clone(),
                },
                TestMiddleware {
                    id: 3,
                    call_order: call_order.clone(),
                },
                mount(
                    "/bar",
                    TestMiddleware {
                        id: 4,
                        call_order: call_order.clone(),
                    },
                ),
                TestMiddleware {
                    id: 5,
                    call_order: call_order.clone(),
                }
            ),
        ));
        app.add(TestMiddleware {
            id: 6,
            call_order: call_order.clone(),
        });

        let req = Request::new(
            Method::Get,
            Uri::from_str("http://localhost/foo/bar").unwrap(),
        );
        let res = Response::default();
        app.build().execute(req, res, background()).wait().unwrap();

        assert_eq!(*call_order.lock().unwrap(), vec![4, 3, 2, 1]);
    }
}
