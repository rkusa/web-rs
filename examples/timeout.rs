#![feature(proc_macro, conservative_impl_trait, generators)]

extern crate ctx;
extern crate futures_await as futures;
extern crate hyper;
extern crate tokio_timer;
extern crate web;

use ctx::background;
use futures::prelude::*;
use futures::future::Either;
use futures::sync::oneshot;
use hyper::server::{Http, Response};
use hyper::StatusCode;
use std::time::Duration;
use tokio_timer::Timer;
use web::*;

type Next = web::Next<Context>;

#[async]
fn handler(_: Request, res: Response, _: Context, _: Next) -> Result<Response, HttpError> {
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
        let timer = Timer::default();
        let sleep = timer.sleep(Duration::from_millis(1000));
        sleep.select2(next(req, res, ctx)).then(|res| match res {
            Ok(Either::A(_)) => Ok(Response::default().with_status(StatusCode::RequestTimeout)),
            Ok(Either::B((res, _))) => Ok(res),
            Err(Either::A((err, _))) => panic!(err),
            Err(Either::B((err, _))) => Err(err),
        })
    });
    app.add(handler);

    let app = app.build();
    let addr = ([127, 0, 0, 1], 3000).into();
    let server = Http::new()
        .bind(&addr, move || Ok(app.handle(|| background())))
        .unwrap();
    println!(
        "Listening on http://{} with 1 thread.",
        server.local_addr().unwrap()
    );
    server.run().unwrap();
}
