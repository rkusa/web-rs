extern crate ctx;
extern crate futures;
extern crate hyper;

pub mod error;
mod helper;
pub use helper::*;

use std::sync::{Arc, Mutex};

use ctx::background;
pub use ctx::Context;
use futures::{future, Future, Poll, Async};
pub use hyper::server::{Request, Response};
use hyper::server::Service;
use hyper::StatusCode;
pub use error::HttpError;

pub enum Respond {
    Next(Request, Response, Context),
    Done(Response),
    Async(Box<Future<Item = Respond, Error = HttpError>>),
    Throw(HttpError),
}

pub use Respond::*;

impl<F> From<F> for Respond
    where F: Future<Item = Respond, Error = HttpError> + 'static
{
    fn from(fut: F) -> Self {
        Respond::Async(Box::new(fut))
    }
}

pub type Middleware = Box<Fn(Request, Response, Context) -> Respond + Send>;

#[derive(Clone)]
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
    curr: Option<Box<Future<Item = Respond, Error = HttpError>>>,
}

impl App {
    pub fn new() -> Self {
        App(Arc::new(Mutex::new(Vec::new())))
    }

    pub fn handle(&self) -> Handle {
        Handle {
            app: self.clone(),
            context: background(),
        }
    }

    pub fn handle_with_context(&self, ctx: Context) -> Handle {
        Handle {
            app: self.clone(),
            context: ctx,
        }
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
        (move |req, res, ctx| self.execute(req, res, ctx).map(|r| r.into()).into()).into()
    }
}

enum Intermediate {
    Next(Request, Response, Context),
    Done(Response),
}

impl Into<Respond> for Intermediate {
    fn into(self) -> Respond {
        match self {
            Intermediate::Done(res) => Respond::Done(res),
            Intermediate::Next(req, res, ctx) => Respond::Next(req, res, ctx),
        }
    }
}

impl Future for Execution {
    type Item = Intermediate;
    type Error = HttpError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let result = if let Some(mut curr) = self.curr.take() {
            match curr.poll() {
                Ok(Async::Ready(result)) => result,
                Ok(Async::NotReady) => {
                    self.curr = Some(curr);
                    return Ok(Async::NotReady)
                },
                Err(err) => return Err(err),
            }
        } else {
            let mws = self.middlewares.lock().unwrap();
            let (req, res, ctx) = self.args.take().unwrap();
            if let Some(mw) = mws.get(self.pos) {
                self.pos += 1;
                mw(req, res, ctx)
            } else {
                return Ok(Async::Ready(Intermediate::Next(req, res, ctx)));
            }
        };

        match result {
            Respond::Next(req, res, ctx) => {
                self.args = Some((req, res, ctx));
                self.poll()
            }
            Respond::Done(res) => Ok(Async::Ready(Intermediate::Done(res))),
            Respond::Async(fut) => {
                self.curr = Some(fut);
                self.poll()
            }
            Respond::Throw(err) => Err(err),
        }
    }
}

pub struct Handle {
    app: App,
    context: Context,
}

impl Service for Handle {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let resp = self.app
            .execute(req, Response::default(), self.context.clone())
            .map(|r| match r {
                     Intermediate::Done(res) => res,
                     _ => HttpError::Status(StatusCode::NotFound).into_response(),
                 })
            .or_else(|err| future::ok(err.into_response()));
        Box::new(resp)
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
