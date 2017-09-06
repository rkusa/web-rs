extern crate ctx;
extern crate web;
extern crate hyper;

use ctx::background;
use web::*;
use hyper::server::{Http, Response};

fn main() {
    let mut app = App::new(|| background());

    app.add(|_, mut res: Response, _, _| {
        res.set_body("Hello World!");
        Ok(res)
    });

    let app = app.build();

    let addr = "127.0.0.1:3000".parse().unwrap();
    // let addr = ([127, 0, 0, 1], 3000).into();

    let server = Http::new().bind(&addr, move || Ok(app.clone())).unwrap();
    println!(
        "Listening on http://{} with 1 thread.",
        server.local_addr().unwrap()
    );
    server.run().unwrap();
}
