extern crate ctx;
extern crate web;
extern crate hyper;

use ctx::background;
use hyper::server::{Http, Response};
use std::thread;
use std::time::Duration;
use web::*;

fn main() {
    let mut app = App::new(|| background());

    let sync = app.offload(|_req, mut res: Response, _ctx, _next| {
        thread::sleep(Duration::from_millis(1000));

        res.set_body("Hello World!");
        Ok(res)
    });
    app.add(sync);

    let app = app.build();
    let addr = "127.0.0.1:3000".parse().unwrap();
    let server = Http::new().bind(&addr, move || Ok(app.clone())).unwrap();
    println!(
        "Listening on http://{} with 1 thread.",
        server.local_addr().unwrap()
    );
    server.run().unwrap();
}
