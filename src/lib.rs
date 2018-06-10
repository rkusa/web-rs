#![feature(fnbox, unboxed_closures, fn_traits)]

extern crate futures;
extern crate http;
extern crate hyper;
#[cfg(feature = "json")]
extern crate serde;
#[cfg(feature = "json")]
extern crate serde_json;

use std::boxed::FnBox;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures::{future, Future, IntoFuture};
use hyper::header::{HeaderValue, CONTENT_TYPE};
use hyper::service::Service;
use hyper::StatusCode;
pub use hyper::{Body, Server};

pub mod error;
pub use error::HttpError;
#[macro_use]
mod helper;
pub use helper::*;

pub type Request = hyper::Request<Body>;
pub type Response = http::response::Builder;
pub type HttpResponse = hyper::Response<Body>;
pub type ResponseResult<E = HttpError> = Result<HttpResponse, E>;

// TODO: maybe use trait alias once https://github.com/rust-lang/rust/issues/41517 lands
// pub type ResponseFuture<E = HttpError> = Future<Item = HttpResponse, Error = E> + Send;
pub type ResponseFuture<E = HttpError> = Box<Future<Item = HttpResponse, Error = E> + Send>;

pub trait Middleware<S>: Send + Sync {
    fn handle(&self, Request, Response, S, Next<S>) -> ResponseFuture;
}

pub trait IntoResponse<E = HttpError> {
    fn into_response(self) -> ResponseFuture<E>;
}

pub struct AppBuilder<S> {
    middlewares: Vec<Box<Middleware<S>>>,
}

pub struct App<S>
where
    S: Send,
{
    middlewares: Arc<Vec<Box<Middleware<S>>>>,
}

pub struct Next<S> {
    pos: usize,
    middlewares: Arc<Vec<Box<Middleware<S>>>>,
    finally: Box<FnBox(Request, Response, S) -> ResponseFuture + Send>,
}

pub struct Serve<S, F>
where
    F: Fn() -> S,
    S: Send,
{
    app: App<S>,
    state_factory: Arc<F>,
}

fn default_fallback<S, E>(_req: Request, _res: Response, _state: S) -> ResponseFuture<E>
where
    E: From<http::Error> + Send + 'static,
{
    let mut res = Response::new();
    res.status(StatusCode::NOT_FOUND);
    Ok(res).into_response()
}

impl<S, F, B> Middleware<S> for F
where
    F: Send + Sync + Fn(Request, Response, S, Next<S>) -> B,
    B: IntoResponse<HttpError>,
{
    fn handle(&self, req: Request, res: Response, state: S, next: Next<S>) -> ResponseFuture {
        let fut = (self)(req, res, state, next).into_response();
        Box::new(fut)
    }
}

impl<S> AppBuilder<S>
where
    S: Send,
{
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

impl<S> Default for AppBuilder<S> {
    fn default() -> Self {
        AppBuilder {
            middlewares: Vec::new(),
        }
    }
}

impl<S> App<S>
where
    S: Send,
{
    pub fn new() -> AppBuilder<S> {
        AppBuilder::default()
    }

    pub fn execute<N>(&self, req: Request, res: Response, state: S, next: N) -> ResponseFuture
    where
        N: FnOnce(Request, Response, S) -> ResponseFuture + Send + 'static,
    {
        let ex = Next {
            pos: 0,
            middlewares: self.middlewares.clone(),
            finally: Box::new(next),
        };
        ex.next(req, res, state)
    }

    pub fn serve<F>(&self, state: F) -> Serve<S, F>
    where
        F: Fn() -> S,
    {
        Serve {
            app: self.clone(),
            state_factory: Arc::new(state),
        }
    }
}

impl<S> Middleware<S> for App<S>
where
    S: Send + 'static,
{
    fn handle(&self, req: Request, res: Response, state: S, next: Next<S>) -> ResponseFuture {
        self.execute(req, res, state, next)
    }
}

impl<S> Clone for App<S>
where
    S: Send,
{
    fn clone(&self) -> Self {
        App {
            middlewares: self.middlewares.clone(),
        }
    }
}

impl<S> Next<S> {
    pub fn new<F>(f: F) -> Self
    where
        F: FnOnce(Request, Response, S) -> ResponseFuture + Send + 'static,
    {
        Next {
            pos: 0,
            middlewares: Arc::new(Vec::new()),
            finally: Box::new(f),
        }
    }
}

impl<S> FnOnce<(Request, Response, S)> for Next<S> {
    type Output = ResponseFuture;

    extern "rust-call" fn call_once(self, args: (Request, Response, S)) -> Self::Output {
        self.next(args.0, args.1, args.2)
    }
}

impl<S> Next<S> {
    fn next(mut self, req: Request, res: Response, state: S) -> ResponseFuture {
        let middlewares = self.middlewares.clone();
        if let Some(mw) = middlewares.get(self.pos) {
            self.pos += 1;
            mw.handle(req, res, state, self)
        } else {
            return (self.finally)(req, res, state);
        }
    }
}

impl<S, F> Service for Serve<S, F>
where
    F: Fn() -> S + Send + Sync + 'static,
    S: Send + 'static,
{
    type ReqBody = Body;
    type ResBody = Body;
    type Error = http::Error;
    type Future = Box<Future<Item = hyper::Response<Self::ResBody>, Error = Self::Error> + Send>;

    fn call(&mut self, req: hyper::Request<Self::ReqBody>) -> Self::Future {
        let state = (self.state_factory)();
        let app = self.app.clone();
        let resp = AssertUnwindSafe(future::lazy(move || {
            app.execute(req, Response::default(), state, default_fallback)
                .or_else(|err| err.into_response())
        }));
        Box::new(resp.catch_unwind().then(|result| match result {
            Ok(res) => res,
            Err(_) => {
                eprintln!("CATCH UNWIND");
                Ok(Response::new()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap())
            }
        }))
    }
}

// TODO: better/shorter name?
pub trait IntoHttpResponse<E=HttpError> {
    fn into_http_response(self) -> ResponseResult<E>;
}

impl<E> IntoHttpResponse<E> for Response
where
    E: From<http::Error> + Send + 'static,
{
    fn into_http_response(mut self) -> ResponseResult<E> {
        self.body(Body::empty()).map_err(E::from)
    }
}

impl<E, P> IntoHttpResponse<E> for hyper::Response<P>
where
    E: From<http::Error> + Send + 'static,
    P: Into<Body>,
{
    fn into_http_response(self) -> ResponseResult<E> {
        let (parts, body) = self.into_parts();
        Ok(hyper::Response::from_parts(parts, body.into()))
    }
}

impl<P, E1, E2> IntoHttpResponse<E1> for Result<hyper::Response<P>, E2>
where
    P: Into<Body>,
    E1: From<E2> + Send + 'static,
{
    fn into_http_response(self) -> ResponseResult<E1> {
        match self {
            Ok(res) => {
                let (parts, body) = res.into_parts();
                let res = hyper::Response::from_parts(parts, body.into());
                Ok(res)
            }
            Err(err) => Err(err.into()),
        }
    }
}

impl<F, I, E1, E2> IntoResponse<E1> for F
where
    F: IntoFuture<Item = I, Error = E2> + Send + 'static,
    I: IntoHttpResponse<E2> + 'static,
    E1: From<E2> + Send + 'static,
    E2: Send + 'static,
    <F as futures::IntoFuture>::Future: Send,
{
    fn into_response(self) -> ResponseFuture<E1> {
        Box::new(
            self.into_future()
                .and_then(I::into_http_response)
                .from_err(),
        )
    }
}

impl<E> IntoHttpResponse<E> for (Response, &'static str)
where
    E: From<http::Error> + Send + 'static,
{
    fn into_http_response(self) -> ResponseResult<E> {
        let (mut res, s) = self;
        res.header(CONTENT_TYPE, HeaderValue::from_str("text/plain").unwrap())
            .body(s.into())
            .map_err(E::from)
    }
}

#[cfg(test)]
mod tests {
    use futures::{Future, IntoFuture};
    use hyper::{self, Body};
    use std::sync::{Arc, Mutex};
    use {
        default_fallback, App, HttpError, HttpResponse, IntoResponse, Middleware, Next as _Next,
        Request, Response, ResponseFuture,
    };

    type Next = _Next<()>;

    #[test]
    fn closure_middleware() {
        let mut app = App::new();
        app.add(|_req, mut res: Response, _state: (), _next| res.body("Hello World!"));
    }

    #[test]
    fn middleware() {
        struct TestMiddleware;

        impl Middleware<()> for TestMiddleware {
            fn handle(
                &self,
                _req: Request,
                res: Response,
                _state: (),
                _next: Next,
            ) -> ResponseFuture {
                Ok::<_, HttpError>(res).into_response()
            }
        }

        let mut app = App::new();
        app.add(TestMiddleware {});
    }

    #[test]
    fn fn_middleware() {
        fn handle(_req: Request, mut res: Response, _state: (), _next: Next) -> ResponseFuture {
            res.body("Hello World!").into_response()
        }

        let mut app = App::new();
        app.add(handle);
    }

    #[test]
    fn http_server() {
        // this test is mainly a reminder that Middlewares need to be Send + Sync
        use hyper::Server;
        let app = App::new().build();
        let addr = "127.0.0.1:3000".parse().unwrap();

        Server::bind(&addr).serve(move || Ok::<_, ::std::io::Error>(app.serve(|| ())));
    }

    #[test]
    fn end_with_done() {
        let mut app = App::<()>::new();
        app.add(|_, res, _, _| Ok::<_, HttpError>(res));
        app.add(|_, res, _, _| Ok::<_, HttpError>((res, "Hello World")));
    }

    #[test]
    fn end_with_next() {
        let mut app = App::new();
        app.add(|req, res, state, next: Next| next(req, res, state));
    }

    #[test]
    fn chain_middleware() {
        let mut app1 = App::<()>::new();
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
            next(req, res, state).inspect(move |_| {
                order1.lock().unwrap().push(2);
            })
        });
        app.add(move |_, res, _, _| {
            order2.lock().unwrap().push(1);
            Ok::<_, HttpError>(res)
        });

        let req = hyper::Request::get("http://localhost")
            .body(Body::empty())
            .unwrap();
        let res = Response::default();
        app.build()
            .execute(req, res, (), default_fallback)
            .wait()
            .unwrap();

        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn return_response_builder() {
        fn handle(
            _req: Request,
            res: Response,
            _state: (),
            _next: Next,
        ) -> Result<Response, HttpError> {
            Ok(res)
        }

        let mut app = App::new();
        app.add(handle);
    }

    #[test]
    fn return_impl_future() {
        fn handle(
            _req: Request,
            mut res: Response,
            _state: (),
            _next: Next,
        ) -> impl Future<Item = HttpResponse, Error = HttpError> {
            res.body(Body::empty()).into_future().from_err()
        }

        let mut app = App::new();
        app.add(handle);
    }

    #[test]
    fn return_impl_into_response() {
        fn handle(_req: Request, res: Response, _state: (), _next: Next) -> impl IntoResponse {
            Ok::<_, HttpError>(res)
        }

        let mut app = App::new();
        app.add(handle);
    }
}
