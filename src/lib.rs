#![feature(fnbox, unboxed_closures, fn_traits)]

extern crate ctx;
extern crate futures;
extern crate hyper;

use std::boxed::FnBox;
use std::sync::Arc;

pub use ctx::{background, Context};
pub use hyper::{Body, Request, Response};
use futures::{future, Future, IntoFuture};
use hyper::header::ContentType;
use hyper::server::Service;
use hyper::StatusCode;

pub mod error;
pub use error::HttpError;
#[macro_use]
mod helper;
pub use helper::*;

pub type WebFuture = Box<Future<Item = Response, Error = HttpError>>;

pub trait Middleware: Send + Sync {
    fn handle(&self, Request, Response, Context, Next) -> WebFuture;
}

impl<F, B> Middleware for F
where
    F: Send + Sync + Fn(Request, Response, Context, Next) -> B,
    B: IntoWebFuture + 'static,
{
    fn handle(&self, req: Request, res: Response, ctx: Context, next: Next) -> WebFuture {
        Box::new((self)(req, res, ctx, next).into_future())
    }
}

pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for (Response, &'static str) {
    fn into_response(self) -> Response {
        let (res, s) = self;
        res.with_body(s).with_header(ContentType::plaintext())
    }
}

pub trait IntoWebFuture {
    fn into_future(self) -> WebFuture;
}

impl<F, I> IntoWebFuture for F
where
    F: IntoFuture<Item = I, Error = HttpError>,
    I: IntoResponse,
    <F as futures::IntoFuture>::Future: 'static,
{
    fn into_future(self) -> WebFuture {
        Box::new(self.into_future().map(|i| i.into_response()))
    }
}

pub fn done(res: Response) -> WebFuture {
    Box::new(future::ok(res))
}

pub struct AppBuilder {
    middlewares: Vec<Box<Middleware>>,
}

impl AppBuilder {
    pub fn new() -> Self {
        AppBuilder {
            middlewares: Vec::new(),
        }
    }

    pub fn add<M>(&mut self, middleware: M)
    where
        M: Middleware + 'static,
    {
        self.middlewares.push(Box::new(middleware));
    }

    pub fn build(self) -> App {
        App {
            middlewares: Arc::new(self.middlewares),
        }
    }
}

#[derive(Clone)]
pub struct App {
    middlewares: Arc<Vec<Box<Middleware>>>,
}

impl App {
    pub fn new() -> AppBuilder {
        AppBuilder::new()
    }

    pub fn execute<N>(&self, req: Request, res: Response, ctx: Context, next: N) -> WebFuture
    where
        N: FnOnce(Request, Response, Context) -> WebFuture + 'static,
    {
        let ex = Next {
            pos: 0,
            middlewares: self.middlewares.clone(),
            finally: Box::new(next),
        };
        ex.next(req, res, ctx)
    }

    // pub fn handle(&self) -> Handle<_> {
    //     self.handle_with_state(|| background())
    // }

    pub fn handle<F>(&self, state: F) -> Handle<F>
    where
        F: Fn() -> Context,
    {
        Handle {
            app: self.clone(),
            state: Arc::new(state),
        }
    }
}

impl Middleware for App {
    fn handle(&self, req: Request, res: Response, ctx: Context, next: Next) -> WebFuture {
        self.execute(req, res, ctx, next)
    }
}

pub struct Next {
    pos: usize,
    middlewares: Arc<Vec<Box<Middleware>>>,
    finally: Box<FnBox(Request, Response, Context) -> WebFuture>,
}

impl Next {
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Request, Response, Context) -> WebFuture + 'static,
    {
        Next {
            pos: 0,
            middlewares: Arc::new(Vec::new()),
            finally: Box::new(f),
        }
    }
}

impl FnOnce<(Request, Response, Context)> for Next {
    type Output = WebFuture;

    extern "rust-call" fn call_once(self, args: (Request, Response, Context)) -> Self::Output {
        self.next(args.0, args.1, args.2)
    }
}

impl Next {
    fn next(mut self, req: Request, res: Response, ctx: Context) -> WebFuture {
        let middlewares = self.middlewares.clone();
        if let Some(mw) = middlewares.get(self.pos) {
            self.pos += 1;
            mw.handle(req, res, ctx, self)
        } else {
            return (self.finally)(req, res, ctx);
        }
    }
}

pub struct Handle<F>
where
    F: Fn() -> Context,
{
    app: App,
    state: Arc<F>,
}

impl<F> Clone for Handle<F>
where
    F: Fn() -> Context,
{
    fn clone(&self) -> Self {
        Handle {
            app: self.app.clone(),
            state: self.state.clone(),
        }
    }
}

impl<F> Service for Handle<F>
where
    F: Fn() -> Context,
{
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let ctx = (self.state)();
        let resp = self.app
            .execute(req, Response::default(), ctx, default_fallback)
            .or_else(|err| future::ok(err.into_response()));
        Box::new(resp)
    }
}

fn default_fallback(_req: Request, _res: Response, _ctx: Context) -> WebFuture {
    done(Response::default().with_status(StatusCode::NotFound))
}

#[cfg(test)]
mod tests {
    use ctx::{background, Context};
    use {default_fallback, done, App, Middleware, Next, WebFuture};
    use hyper::{Request, Response};
    use hyper::{Method, Uri};
    use std::str::FromStr;
    use futures::Future;
    use std::sync::{Arc, Mutex};

    #[test]
    fn closure_middleware() {
        let mut app = App::new();
        app.add(|req, mut res: Response, ctx, next: Next| {
            res.set_body("Hello World!");
            next(req, res, ctx)
        });
    }

    #[test]
    fn middleware() {
        struct TestMiddleware;

        impl Middleware for TestMiddleware {
            fn handle(
                &self,
                _req: Request,
                res: Response,
                _ctx: Context,
                _next: Next,
            ) -> WebFuture {
                done(res)
            }
        }

        let mut app = App::new();
        app.add(TestMiddleware {});
    }

    #[test]
    fn fn_middleware() {
        fn handle(req: Request, mut res: Response, ctx: Context, next: Next) -> WebFuture {
            res.set_body("Hello World!");
            next(req, res, ctx)
        }

        let mut app = App::new();
        app.add(handle);
    }

    #[test]
    fn http_server() {
        // this test is mainly a reminder that Middlewares need to be Send + Sync
        use hyper::server::Http;
        let app = App::new().build();
        let addr = "127.0.0.1:3000".parse().unwrap();
        Http::new()
            .bind(&addr, move || Ok(app.handle(|| background())))
            .unwrap();
    }

    #[test]
    fn end_with_done() {
        let mut app = App::new();
        app.add(|_, res, _, _| Ok(res));
        app.add(|_, res, _, _| Ok((res, "Hello World")));
    }

    #[test]
    fn end_with_next() {
        let mut app = App::new();
        app.add(|req, res, ctx, next: Next| next(req, res, ctx));
    }

    #[test]
    fn chain_middleware() {
        let mut app1 = App::new();
        let app2 = App::new();
        app1.add(app2.build());
    }

    #[test]
    fn after_next() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let order1 = order.clone();
        let order2 = order.clone();

        let mut app = App::new();
        app.add(move |req, res, ctx, next: Next| {
            let order1 = order1.clone();
            next(req, res, ctx).inspect(move |_| { order1.lock().unwrap().push(2); })
        });
        app.add(move |_, res, _, _| {
            order2.lock().unwrap().push(1);
            done(res)
        });

        let req = Request::new(Method::Get, Uri::from_str("http://localhost").unwrap());
        let res = Response::default();
        app.build()
            .execute(req, res, background(), default_fallback)
            .wait()
            .unwrap();

        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }
}
