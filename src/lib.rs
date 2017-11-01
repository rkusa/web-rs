#![feature(fnbox, unboxed_closures, fn_traits)]

extern crate ctx;
extern crate futures;
extern crate hyper;
#[cfg(feature = "json")]
extern crate serde;
#[cfg(feature = "json")]
extern crate serde_json;

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

pub trait Middleware<S>: Send + Sync {
    fn handle(&self, Request, Response, S, Next<S>) -> WebFuture;
}

impl<S, F, B> Middleware<S> for F
where
    F: Send + Sync + Fn(Request, Response, S, Next<S>) -> B,
    B: IntoWebFuture + 'static,
{
    fn handle(&self, req: Request, res: Response, state: S, next: Next<S>) -> WebFuture {
        Box::new((self)(req, res, state, next).into_future())
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

pub struct AppBuilder<S> {
    middlewares: Vec<Box<Middleware<S>>>,
}

impl<S> AppBuilder<S> {
    pub fn new() -> Self {
        AppBuilder {
            middlewares: Vec::new(),
        }
    }

    pub fn add<M>(&mut self, middleware: M)
    where
        M: Middleware<S> + 'static,
    {
        self.middlewares.push(Box::new(middleware));
    }

    pub fn build(self) -> App<S> {
        App {
            middlewares: Arc::new(self.middlewares),
        }
    }
}

pub struct App<S> {
    middlewares: Arc<Vec<Box<Middleware<S>>>>,
}

impl<S> Clone for App<S> {
    fn clone(&self) -> Self {
        App {
            middlewares: self.middlewares.clone(),
        }
    }
}

impl<S> App<S> {
    pub fn new() -> AppBuilder<S> {
        AppBuilder::new()
    }

    pub fn execute<N>(&self, req: Request, res: Response, state: S, next: N) -> WebFuture
    where
        N: FnOnce(Request, Response, S) -> WebFuture + 'static,
    {
        let ex = Next {
            pos: 0,
            middlewares: self.middlewares.clone(),
            finally: Box::new(next),
        };
        ex.next(req, res, state)
    }

    // pub fn handle(&self) -> Handle<_> {
    //     self.handle_with_state(|| background())
    // }

    pub fn handle<F>(&self, state: F) -> Handle<S, F>
    where
        F: Fn() -> S,
    {
        Handle {
            app: self.clone(),
            state: Arc::new(state),
        }
    }
}

impl<S> Middleware<S> for App<S>
where
    S: 'static,
    {
    fn handle(&self, req: Request, res: Response, state: S, next: Next<S>) -> WebFuture {
        self.execute(req, res, state, next)
    }
}

pub struct Next<S> {
    pos: usize,
    middlewares: Arc<Vec<Box<Middleware<S>>>>,
    finally: Box<FnBox(Request, Response, S) -> WebFuture>,
}

impl<S> Next<S> {
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Request, Response, S) -> WebFuture + 'static,
    {
        Next {
            pos: 0,
            middlewares: Arc::new(Vec::new()),
            finally: Box::new(f),
        }
    }
}

impl<S> FnOnce<(Request, Response, S)> for Next<S> {
    type Output = WebFuture;

    extern "rust-call" fn call_once(self, args: (Request, Response, S)) -> Self::Output {
        self.next(args.0, args.1, args.2)
    }
}

impl<S> Next<S> {
    fn next(mut self, req: Request, res: Response, state: S) -> WebFuture {
        let middlewares = self.middlewares.clone();
        if let Some(mw) = middlewares.get(self.pos) {
            self.pos += 1;
            mw.handle(req, res, state, self)
        } else {
            return (self.finally)(req, res, state);
        }
    }
}

pub struct Handle<S, F>
where
    F: Fn() -> S,
{
    app: App<S>,
    state: Arc<F>,
}

impl<S, F> Clone for Handle<S, F>
where
    F: Fn() -> S,
{
    fn clone(&self) -> Self {
        Handle {
            app: self.app.clone(),
            state: self.state.clone(),
        }
    }
}

impl<S, F> Service for Handle<S, F>
where
    S: 'static,
    F: Fn() -> S,
{
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let state = (self.state)();
        let resp = self.app
            .execute(req, Response::default(), state, default_fallback)
            .or_else(|err| future::ok(err.into_response()));
        Box::new(resp)
    }
}

fn default_fallback<S>(_req: Request, _res: Response, _state: S) -> WebFuture {
    done(Response::default().with_status(StatusCode::NotFound))
}

#[cfg(test)]
mod tests {
    use ctx::{background, Context};
    use {default_fallback, done, App, Middleware, Next as _Next, WebFuture};
    use hyper::{Request, Response};
    use hyper::{Method, Uri};
    use std::str::FromStr;
    use futures::Future;
    use std::sync::{Arc, Mutex};

    type Next = _Next<Context>;

    #[test]
    fn closure_middleware() {
        let mut app = App::new();
        app.add(|req, mut res: Response, state, next: Next| {
            res.set_body("Hello World!");
            next(req, res, state)
        });
    }

    #[test]
    fn middleware() {
        struct TestMiddleware;

        impl Middleware<Context> for TestMiddleware {
            fn handle(
                &self,
                _req: Request,
                res: Response,
                _state: Context,
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
        fn handle(req: Request, mut res: Response, state: Context, next: Next) -> WebFuture {
            res.set_body("Hello World!");
            next(req, res, state)
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
        let mut app = App::<Context>::new();
        app.add(|_, res, _, _| Ok(res));
        app.add(|_, res, _, _| Ok((res, "Hello World")));
    }

    #[test]
    fn end_with_next() {
        let mut app = App::new();
        app.add(|req, res, state, next: Next| next(req, res, state));
    }

    #[test]
    fn chain_middleware() {
        let mut app1 = App::<Context>::new();
        let app2 = App::new();
        app1.add(app2.build());
    }

    #[test]
    fn after_next() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let order1 = order.clone();
        let order2 = order.clone();

        let mut app = App::new();
        app.add(move |req, res, state, next: Next| {
            let order1 = order1.clone();
            next(req, res, state).inspect(move |_| { order1.lock().unwrap().push(2); })
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
