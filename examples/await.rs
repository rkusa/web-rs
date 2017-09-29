#![feature(proc_macro, conservative_impl_trait, generators)]

extern crate ctx;
extern crate futures_await as futures;
extern crate hyper;
extern crate tokio_timer;
extern crate web;

use ctx::background;
use futures::prelude::*;
use hyper::server::{Http, Response};
use hyper::StatusCode;
use std::time::Duration;
use tokio_timer::Timer;
use web::*;

type Next = web::Next<Context>;

#[async]
fn handler(_: Request, mut res: Response, _: Context, _: Next) -> Result<Response, HttpError> {
    // Set a timeout that expires in 100 milliseconds
    let timer = Timer::default();
    let sleep = timer.sleep(Duration::from_millis(100));

    if let Err(_) = await!(sleep) {
        return Err(StatusCode::RequestTimeout.into());
    }

    res.set_body("Hello World!");
    Ok(res)
}

fn main() {
    let mut app = App::new();

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
