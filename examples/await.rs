#![feature(generators)]

extern crate futures_await as futures;
extern crate hyper;
extern crate tokio_timer;
extern crate web;

use futures::prelude::{async, await, Future};
use hyper::StatusCode;
use std::time::Duration;
use tokio_timer::sleep;
use web::*;

type Next = web::Next<()>;

#[async]
fn handler(_: Request, mut res: Response, _: (), _: Next) -> ResponseResult {
    // Set a timeout that expires in 100 milliseconds
    let sleep = sleep(Duration::from_millis(100));

    if let Err(_) = await!(sleep) {
        return Err(Error::from(HttpError::Status(StatusCode::REQUEST_TIMEOUT)));
    }

    res.body("Hello World!").into_http_response()
}

fn main() {
    let mut app = App::new();

    app.add(handler);

    let app = app.build();
    let addr = ([127, 0, 0, 1], 3000).into();
    let server =
        hyper::Server::bind(&addr).serve(move || Ok::<_, ::std::io::Error>(app.serve(|| ())));
    println!("Listening on http://{} with 1 thread.", server.local_addr());
    hyper::rt::run(server.map_err(|e| {
        eprintln!("Server Error: {}", e);
    }));
}
