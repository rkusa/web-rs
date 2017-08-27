extern crate ctx;
extern crate web;
extern crate hyper;

use ctx::background;
use web::*;
use hyper::server::Http;

fn main() {
    let mut app = App::new(|| background());

    app.handler(|_req, mut res, _ctx| {
      res.set_body("Hello World!");
      // Ok(Some(ctx))
      Ok(None)
    }.into());

    let addr = "127.0.0.1:3000".parse().unwrap();
    // let addr = ([127, 0, 0, 1], 3000).into();

    let server = Http::new().bind(&addr, move || Ok(app.clone())).unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run().unwrap();
}