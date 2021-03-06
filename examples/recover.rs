extern crate futures;
extern crate hyper;
extern crate web;

use futures::Future;
use web::*;

fn main() {
    let mut app = App::new();

    app.add(|_req, _res, _ctx, _next| {
        panic!("should recover");
        #[allow(unreachable_code)]
        Ok::<_, HttpError>(_res)
    });

    let app = app.build();
    let addr = ([127, 0, 0, 1], 3000).into();
    let server =
        hyper::Server::bind(&addr).serve(move || Ok::<_, ::std::io::Error>(app.serve(|| ())));
    println!("Listening on http://{} with 1 thread.", server.local_addr());
    hyper::rt::run(server.map_err(|e| {
        eprintln!("Server Error: {}", e);
    }));
}
