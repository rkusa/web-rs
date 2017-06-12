extern crate hyper;
extern crate web;

use hyper::server::{Http, Response};
use web::App;
use web::Respond::*;

fn main() {
    let mut app = App::new();

    app.attach(|req, _, ctx| {
                   let res = Response::new().with_body("Hello World!");
                   Done(req, res, ctx)
               });

    let addr = "127.0.0.1:3000".parse().unwrap();
    // let addr = ([127, 0, 0, 1], 3000).into();

    let server = Http::new().bind(&addr, app).unwrap();
    println!("Listening on http://{} with 1 thread.",
             server.local_addr().unwrap());
    server.run().unwrap();
}