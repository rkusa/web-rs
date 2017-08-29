extern crate ctx;
extern crate web;
extern crate hyper;
extern crate tokio_timer;
extern crate futures;

use ctx::background;
use web::*;
use hyper::server::{Http, Response};
use tokio_timer::Timer;
use std::time::Duration;
use futures::future::Future;

fn main() {
    let mut app = App::new(|| background());

    app.add(|_req, mut res: Response, _ctx| {
      let timer = Timer::default();

      // Set a timeout that expires in 500 milliseconds
      let sleep = timer.sleep(Duration::from_millis(100));
      sleep.wait().unwrap();

      res.set_body("Hello World!");
      Ok(res)
    });

    let addr = "127.0.0.1:3000".parse().unwrap();
    // let addr = ([127, 0, 0, 1], 3000).into();

    let server = Http::new().bind(&addr, move || Ok(app.clone())).unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run().unwrap();
}
