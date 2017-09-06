extern crate ctx;
extern crate futures;
extern crate hyper;
extern crate tokio_timer;
extern crate web;

use ctx::background;
use futures::future::Future;
use hyper::server::{Http, Response};
use hyper::StatusCode;
use std::time::Duration;
use tokio_timer::Timer;
use web::*;

fn main() {
    let mut app = App::new(|| background());

    app.add(|_req, mut res: Response, _ctx, _next| {
        // Set a timeout that expires in 500 milliseconds
        let timer = Timer::default();
        let sleep = timer.sleep(Duration::from_millis(100));

        sleep
            .map_err(|_| StatusCode::RequestTimeout.into())
            .and_then(|_| {
                res.set_body("Hello World!");
                Ok(res)
            })
    });

    let app = app.build();
    let addr = ([127, 0, 0, 1], 3000).into();
    let server = Http::new().bind(&addr, move || Ok(app.clone())).unwrap();
    println!(
        "Listening on http://{} with 1 thread.",
        server.local_addr().unwrap()
    );
    server.run().unwrap();
}
