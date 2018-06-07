#![feature(proc_macro, generators)]

extern crate futures_await as futures;
extern crate hyper;
extern crate tokio_timer;
extern crate web;

use futures::future::Either;
use futures::prelude::*;
use futures::sync::oneshot;
use hyper::{Body, StatusCode};
use std::time::Duration;
use tokio_timer::sleep;
use web::*;

type Next = web::Next<()>;

#[async]
fn handler(_: Request, res: Response, _: (), _: Next) -> Result<Response, HttpError> {
    // for the purpose of this example, create a future that will never resolve
    let (c, p) = oneshot::channel::<i32>();
    await!(p).unwrap();
    c.send(3).unwrap();

    Ok(res)
}

fn main() {
    let mut app = App::new();

    // add a 1000ms timeout middleware
    app.add(|req, res, ctx, next: Next| {
        let sleep = sleep(Duration::from_millis(1000));
        sleep.select2(next(req, res, ctx)).then(|res| match res {
            Ok(Either::A(_)) => Response::new()
                .status(StatusCode::REQUEST_TIMEOUT)
                .body(Body::empty())
                .into_response(),
            Ok(Either::B((res, _))) => Ok(res),
            Err(Either::A((err, _))) => panic!(err),
            Err(Either::B((err, _))) => Err(err),
        })
    });
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
