// TODOs:
// - Error Handling

extern crate futures;
extern crate hyper;

mod error;
mod helper;
pub use helper::*;

use std::rc::Rc;
use std::cell::RefCell;

use futures::{future, Future, Poll, Async};
use hyper::server::{Service, Request, Response};
use hyper::status::StatusCode;
pub use error::Error;

pub enum Respond {
    Next(Request),
    Done(Request, Response),
    Async(Box<Future<Item = Respond, Error = Error>>),
}

impl<F> From<F> for Respond
    where F: 'static + Future<Item = Respond, Error = Error>
{
    fn from(fut: F) -> Self {
        Respond::Async(Box::new(fut))
    }
}

pub type Middleware = Box<Fn(Request) -> Respond>;

pub struct App(Rc<RefCell<Vec<Middleware>>>);

impl<F> From<F> for Middleware
    where F: 'static + Fn(Request) -> Respond
{
    fn from(middleware: F) -> Middleware {
        Box::new(middleware)
    }
}

struct Execution {
    req: Option<Request>,
    pos: usize,
    middlewares: Rc<RefCell<Vec<Middleware>>>,
    curr: Option<Box<Future<Item = Respond, Error = Error>>>,
}

impl App {
    pub fn new() -> Self {
        App(Rc::new(RefCell::new(Vec::new())))
    }

    pub fn attach<F>(&mut self, middleware: F)
        where F: Into<Middleware>
    {
        self.0.borrow_mut().push(middleware.into());
    }

    fn execute(&self, req: Request) -> Execution {
        Execution {
            pos: 0,
            middlewares: self.0.clone(),
            curr: None,
            req: Some(req),
        }
    }

    pub fn middleware(self) -> Middleware {
        Box::new(move |req| {
            self.execute(req)
                .map(|(req, res)| if let Some(res) = res {
                         Respond::Done(req, res)
                     } else {
                         Respond::Next(req)
                     })
                .into()
        })
    }
}

impl Future for Execution {
    type Item = (Request, Option<Response>);
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let result = if let Some(mut curr) = self.curr.take() {
            match curr.poll() {
                Ok(Async::Ready(result)) => result,
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(err) => return Err(err),
            }
        } else {
            let mws = self.middlewares.borrow();
            if let Some(mw) = mws.get(self.pos) {
                self.pos += 1;
                mw(self.req.take().unwrap())
            } else {
                return Ok(Async::Ready((self.req.take().unwrap(), None)));
            }
        };

        match result {
            Respond::Next(req) => {
                self.req = Some(req);
                self.poll()
            }
            Respond::Done(req, res) => Ok(Async::Ready((req, Some(res)))),
            Respond::Async(fut) => {
                self.curr = Some(fut);
                self.poll()
            }
        }
    }
}

impl Service for App {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let default = || Response::new().with_status(StatusCode::NotFound);
        let resp = self.execute(req)
            .map(|(_req, res)| res.unwrap_or_else(default))
            .or_else(|err| future::ok(err.into_response()));
        Box::new(resp)
    }
}

#[cfg(test)]
mod tests {
    use App;
    use Respond::*;
    use futures::Future;
    use hyper::server::Request;
    use hyper::{Method, Uri};
    use std::str::FromStr;

    #[test]
    fn it_works() {
        let mut app = App::new();
        app.attach(|req| Next(req));

        let req = Request::new(Method::Get, Uri::from_str("http://localhost").unwrap());
        let result = app.execute(req).wait();
        assert!(result.unwrap().1.is_none());
    }

    #[test]
    fn middleware() {
        let mut app = App::new();
        app.attach(App::new().middleware());
    }
}
