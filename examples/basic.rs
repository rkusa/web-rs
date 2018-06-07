extern crate futures;
extern crate hyper;
extern crate web;

use futures::Future;
use web::*;

fn main() {
    let mut app = App::new();

    app.add(|_req, mut res: Response, _ctx, _next| done(res.body("Hello World!")));

    let app = app.build();
    let addr = ([127, 0, 0, 1], 3000).into();
    let server =
        hyper::Server::bind(&addr).serve(move || Ok::<_, ::std::io::Error>(app.serve(|| ())));
    println!("Listening on http://{}", server.local_addr());
    hyper::rt::run(server.map_err(|e| {
        eprintln!("Server Error: {}", e);
    }));
}
