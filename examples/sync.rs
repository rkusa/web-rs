extern crate ctx;
extern crate web;
extern crate hyper;

use std::thread;
use ctx::background;
use web::*;
use hyper::server::{Http, Response};
use std::time::Duration;

fn main() {
    let mut app = App::new(|| background());

    app.add_sync(|_req, mut res: Response, _ctx| {
      thread::sleep(Duration::from_millis(1000));

      res.set_body("Hello World!");
      Ok(res)
    });

    let addr = "127.0.0.1:3000".parse().unwrap();
    // let addr = ([127, 0, 0, 1], 3000).into();

    let server = Http::new().bind(&addr, move || Ok(app.clone())).unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run().unwrap();
}
