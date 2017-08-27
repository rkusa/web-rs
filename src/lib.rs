extern crate ctx;
extern crate futures;
extern crate futures_cpupool;
extern crate hyper;

pub mod error;
// mod helper;
// pub use helper::*;

use std::sync::{Arc, Mutex, RwLock};

// use ctx::background;
pub use ctx::Context;
use futures_cpupool::{CpuPool, CpuFuture};
pub use hyper::server::{Request, Response};
use hyper::server::Service;
// use hyper::StatusCode;
pub use error::HttpError;
use std::ops::Deref;

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

pub type WebResult = Result<Respond, HttpError>;

pub trait Middleware: Send + Sync {
    fn handle(&self, Request, Response, Context) -> WebResult;
    fn after(&self) {}
}

impl Middleware for Box<Middleware> {
    fn handle(&self, req: Request, res: Response, ctx: Context) -> WebResult {
        self.deref().handle(req, res, ctx)
    }

    fn after(&self) {
        self.deref().after()
    }
}

pub struct FnMiddleware<F>
where
    F: Fn(Request, Response, Context) -> WebResult + Send + Sync,
{
    func: F,
}

impl<F> FnMiddleware<F> where
    F: Fn(Request, Response, Context) -> WebResult + Send + Sync,
    {
    pub fn new(f: F) -> Self {
        FnMiddleware{ func: f}
    }
}

impl<F> Middleware for FnMiddleware<F>
where
    F: Fn(Request, Response, Context) -> WebResult
        + Send
        + Sync,
{
    fn handle(&self, req: Request, res: Response, ctx: Context) -> WebResult {
        (self.func)(req, res, ctx)
    }
}

impl<F> From<F> for Box<Middleware>
where
    F: 'static
        + Fn(Request, Response, Context) -> WebResult
        + Send
        + Sync,
{
    fn from(f: F) -> Self {
        Box::new(FnMiddleware { func: f })
    }
}

pub struct App<F>
where
    F: Fn() -> Context + Send + 'static,
{
    middlewares: Arc<RwLock<Vec<Box<Middleware>>>>,
    pool: CpuPool,
    context: Arc<Mutex<F>>,
}

impl<F> Clone for App<F>
where
    F: Fn() -> Context + Send + 'static,
{
    fn clone(&self) -> Self {
        App {
            middlewares: self.middlewares.clone(),
            pool: self.pool.clone(),
            context: self.context.clone(),
        }
    }
}

impl<F> App<F>
where
    F: Fn() -> Context + Send + 'static,
{
    pub fn new(ctx: F) -> Self {
        App {
            middlewares: Arc::new(RwLock::new(Vec::new())),
            pool: CpuPool::new(32),
            context: Arc::new(Mutex::new(ctx)),
        }
    }

    pub fn attach<M>(&mut self, middleware: M)
    where
        M: Middleware + 'static,
    {
        self.middlewares.write().unwrap().push(Box::new(middleware));
    }

    pub fn handler<T>(&mut self, handler: T)
    where
        T: 'static + Fn(Request, Response, Context) -> WebResult + Send + Sync,
    {
        self.middlewares.write().unwrap().push(
            Box::new(FnMiddleware {
                func: handler,
            }),
        );
    }
}

impl<F> Middleware for App<F>
where
    F: Fn() -> Context + Send + 'static,
{
    fn handle(&self, req: Request, res: Response, ctx: Context) -> WebResult {
        let middlewares = self.middlewares.read().unwrap();
        let mut iter = middlewares.iter();

        let mut res = res;
        let mut req_ctx = Some((req, ctx));

        while let Some(mw) = iter.next() {
            let (req, ctx) = req_ctx.take().unwrap();
            match mw.handle(req, res, ctx) {
                Ok(Next(rq, rs, cx)) => {
                    res = rs;
                    req_ctx = Some((rq, cx));
                },
                Ok(Done(rs)) => {
                    res = rs;
                    break;
                },
                Err(err) => return Err(err),
            }
        }

        while let Some(mw) = iter.next_back() {
            mw.after();
        }

        if let Some((req, ctx)) = req_ctx.take() {
            Ok(Next(req, res, ctx))
        } else {
            Ok(Done(res))
        }
    }
}

impl<F> Service for App<F>
where
    F: Fn() -> Context + Send + 'static,
{
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = CpuFuture<Self::Response, Self::Error>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let ctx = self.context.clone();
        let middlewares = self.middlewares.clone();
        self.pool.spawn_fn(move || {
            let ctx = (ctx.lock().unwrap())();
            let mut res = Response::default();
            let middlewares = middlewares.read().unwrap();
            let mut iter = middlewares.iter();

            let mut req_ctx = Some((req, ctx));

            while let Some(mw) = iter.next() {
                let (req, ctx) = req_ctx.take().unwrap();
                match mw.handle(req, res, ctx) {
                    Ok(Next(rq, rs, cx)) => {
                        res = rs;
                        req_ctx = Some((rq, cx));
                    },
                    Ok(Done(rs)) => {
                        res = rs;
                        break;
                    },
                    Err(err) => {
                        res = err.into_response();
                        break;
                    },
                }
            }

            while let Some(mw) = iter.next_back() {
                mw.after();
            }

            // self.execute(req, res, ctx);
            Ok(res)
        })
    }
}

#[cfg(test)]
mod tests {
    use ctx::background;
    use App;
    // use hyper::server::{Request, Response};
    // use hyper::{Method, Uri};
    // use std::str::FromStr;

    #[test]
    fn handler() {
        let mut app = App::new(|| background());
        app.handler(|_req, _res, _ctx| Ok(None));

        // let req = Request::new(Method::Get, Uri::from_str("http://localhost").unwrap());
        // let res = Response::default();
        // let result = app.handle(&req, &res, background());
        // assert_eq!(result.unwrap().3, false);
    }

    #[test]
    fn middleware() {
        let mut app1 = App::new(|| background());
        let app2 = App::new(|| background());
        app1.attach(app2);
    }
}
