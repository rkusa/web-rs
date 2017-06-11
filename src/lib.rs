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
    Next(Request, Response),
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

pub type Middleware = Box<Fn(Request, Response) -> Respond>;

pub struct App(Rc<RefCell<Vec<Middleware>>>);

impl<F> From<F> for Middleware
    where F: 'static + Fn(Request, Response) -> Respond
{
    fn from(middleware: F) -> Middleware {
        Box::new(middleware)
    }
}

struct Execution {
    req: Option<Request>,
    res: Option<Response>,
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

    fn execute(&self, req: Request, res: Response) -> Execution {
        Execution {
            req: Some(req),
            res: Some(res),
            pos: 0,
            middlewares: self.0.clone(),
            curr: None,
        }
    }

    pub fn middleware(self) -> Middleware {
        Box::new(move |req, res| {
            self.execute(req, res)
                .map(|(req, res, handled)| if handled {
                         Respond::Done(req, res)
                     } else {
                         Respond::Next(req, res)
                     })
                .into()
        })
    }
}

impl Future for Execution {
    type Item = (Request, Response, bool);
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
                mw(self.req.take().unwrap(), self.res.take().unwrap())
            } else {
                return Ok(Async::Ready((self.req.take().unwrap(),
                                        self.res.take().unwrap(),
                                        false)));
            }
        };

        match result {
            Respond::Next(req, res) => {
                self.req = Some(req);
                self.res = Some(res);
                self.poll()
            }
            Respond::Done(req, res) => Ok(Async::Ready((req, res, true))),
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
        let resp = self.execute(req, Response::default())
            .map(|(_, res, handled)| if handled {
                     res
                 } else {
                     Response::new().with_status(StatusCode::NotFound)
                 })
            .or_else(|err| future::ok(err.into_response()));
        Box::new(resp)
    }
}

#[cfg(test)]
mod tests {
    use App;
    use Respond::*;
    use futures::Future;
    use hyper::server::{Request, Response};
    use hyper::{Method, Uri};
    use std::str::FromStr;

    #[test]
    fn it_works() {
        let mut app = App::new();
        app.attach(|req, res| Next(req, res));

        let req = Request::new(Method::Get, Uri::from_str("http://localhost").unwrap());
        let res = Response::default();
        let result = app.execute(req, res).wait();
        assert_eq!(result.unwrap().2, false);
    }

    #[test]
    fn middleware() {
        let mut app = App::new();
        app.attach(App::new().middleware());
    }
}
