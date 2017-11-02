extern crate ctx;
extern crate hyper;
extern crate web;

use ctx::background;
use hyper::server::Http;
use web::*;

fn main() {
    let mut app = App::new();

    app.add(|_req, _res, _ctx, _next| {
        panic!("should recover");
        #[allow(unreachable_code)]
        Ok(_res)
    });

    let app = app.build();
    let addr = ([127, 0, 0, 1], 3000).into();
    let server = Http::new()
        .bind(&addr, move || Ok(app.handle(|| background())))
        .unwrap();
    println!(
        "Listening on http://{} with 1 thread.",
        server.local_addr().unwrap()
    );
    server.run().unwrap();
}
