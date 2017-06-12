extern crate ctx;
extern crate futures;
extern crate hyper;

pub mod error;
mod helper;
pub use helper::*;

use std::io;
use std::sync::{Arc, Mutex};
use std::net::SocketAddr;

use ctx::background;
pub use ctx::Context;
use futures::{future, Future, Poll, Async};
pub use hyper::server::{Request, Response};
use hyper::server::{Service, NewService, Http, Server};
use hyper::status::StatusCode;
pub use error::Error;

pub enum Respond {
    Next(Request, Response, Context),
    Done(Request, Response, Context),
    Async(Box<Future<Item = Respond, Error = Error>>),
    Error(Error),
}

pub use Respond::*;

impl<F> From<F> for Respond
    where F: Future<Item = Respond, Error = Error> + 'static
{
    fn from(fut: F) -> Self {
        Respond::Async(Box::new(fut))
    }
}

pub type Middleware = Box<Fn(Request, Response, Context) -> Respond + Send>;

pub struct App(Arc<Mutex<Vec<Middleware>>>);

impl<F> From<F> for Middleware
    where F: Fn(Request, Response, Context) -> Respond + Send + 'static
{
    fn from(middleware: F) -> Middleware {
        Box::new(middleware)
    }
}

struct Execution {
    args: Option<(Request, Response, Context)>,
    pos: usize,
    middlewares: Arc<Mutex<Vec<Middleware>>>,
    curr: Option<Box<Future<Item = Respond, Error = Error>>>,
}

impl App {
    pub fn new() -> Self {
        App(Arc::new(Mutex::new(Vec::new())))
    }

    pub fn attach<F>(&mut self, middleware: F)
        where F: Into<Middleware>
    {
        self.0.lock().unwrap().push(middleware.into());
    }

    fn execute(&self, req: Request, res: Response, ctx: Context) -> Execution {
        Execution {
            args: Some((req, res, ctx)),
            pos: 0,
            middlewares: self.0.clone(),
            curr: None,
        }
    }

    pub fn middleware(self) -> Middleware {
        Box::new(move |req, res, ctx| {
            self.execute(req, res, ctx)
                .map(|(req, res, ctx, handled)| if handled {
                         Respond::Done(req, res, ctx)
                     } else {
                         Respond::Next(req, res, ctx)
                     })
                .into()
        })
    }

    pub fn server(self, addr: &SocketAddr) -> Result<Server<App, hyper::Body>, hyper::Error> {
        Http::new().bind(&addr, self)
    }
}

impl Future for Execution {
    type Item = (Request, Response, Context, bool);
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let result = if let Some(mut curr) = self.curr.take() {
            match curr.poll() {
                Ok(Async::Ready(result)) => result,
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(err) => return Err(err),
            }
        } else {
            let mws = self.middlewares.lock().unwrap();
            let (req, res, ctx) = self.args.take().unwrap();
            if let Some(mw) = mws.get(self.pos) {
                self.pos += 1;
                mw(req, res, ctx)
            } else {
                return Ok(Async::Ready((req, res, ctx, false)));
            }
        };

        match result {
            Respond::Next(req, res, ctx) => {
                self.args = Some((req, res, ctx));
                self.poll()
            }
            Respond::Done(req, res, ctx) => Ok(Async::Ready((req, res, ctx, true))),
            Respond::Async(fut) => {
                self.curr = Some(fut);
                self.poll()
            }
            Respond::Error(err) => Err(err),
        }
    }
}

impl Service for App {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let resp = self.execute(req, Response::default(), background())
            .map(|(_, res, _, handled)| if handled {
                     res
                 } else {
                     Error::Status(StatusCode::NotFound).into_response()
                 })
            .or_else(|err| future::ok(err.into_response()));
        Box::new(resp)
    }
}

impl NewService for App {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Instance = App;

    fn new_service(&self) -> Result<Self::Instance, io::Error> {
        Ok(App(self.0.clone()))
    }
}

#[cfg(test)]
mod tests {
    use ctx::background;
    use App;
    use Respond::*;
    use futures::Future;
    use hyper::server::{Request, Response};
    use hyper::{Method, Uri};
    use std::str::FromStr;

    #[test]
    fn it_works() {
        let mut app = App::new();
        app.attach(|req, res, ctx| Next(req, res, ctx));

        let req = Request::new(Method::Get, Uri::from_str("http://localhost").unwrap());
        let res = Response::default();
        let result = app.execute(req, res, background()).wait();
        assert_eq!(result.unwrap().3, false);
    }

    #[test]
    fn middleware() {
        let mut app = App::new();
        app.attach(App::new().middleware());
    }
}
