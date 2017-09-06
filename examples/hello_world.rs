extern crate ctx;
extern crate hyper;
extern crate web;

use ctx::background;
use hyper::server::{Http, Response};
use web::*;

fn main() {
    let mut app = App::new(|| background());

    app.add(|_req, mut res: Response, _ctx, _next| {
        res.set_body("Hello World!");
        Ok(res)
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
