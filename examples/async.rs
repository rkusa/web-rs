extern crate web;
extern crate hyper;
extern crate tokio_timer;
extern crate futures;

use web::*;
use hyper::server::Http;
use hyper::StatusCode;
use tokio_timer::Timer;
use std::time::Duration;
use futures::future::Future;

fn main() {
    let mut app = App::new();

    app.attach(|_, _, _| {
        let timer = Timer::default();

        // Set a timeout that expires in 500 milliseconds
        let sleep = timer.sleep(Duration::from_millis(500));

        let delay = sleep
            .map(|_| {
                let res = Response::new().with_body("Hello World!");
                Done(res)
            })
            .map_err(|err| {
                println!("TimerError {:?}", err);
                StatusCode::InternalServerError.into()
            });

        delay.into()
    });

    let addr = "127.0.0.1:3000".parse().unwrap();
    // let addr = ([127, 0, 0, 1], 3000).into();

    let server = Http::new().bind(&addr, move || Ok(app.handle())).unwrap();
    println!(
        "Listening on http://{} with 1 thread.",
        server.local_addr().unwrap()
    );
    server.run().unwrap();
}
