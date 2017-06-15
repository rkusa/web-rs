extern crate web;
extern crate hyper;

use web::*;
use hyper::server::Http;

fn main() {
    let mut app = App::new();

    app.attach(|req, _, ctx| {
                   let res = Response::new().with_body("Hello World!");
                   Done(res)
               });

    let addr = "127.0.0.1:3000".parse().unwrap();
    // let addr = ([127, 0, 0, 1], 3000).into();

    let server = Http::new().bind(&addr, move || Ok(app.handle())).unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run().unwrap();
}