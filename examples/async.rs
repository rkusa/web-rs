extern crate futures;
extern crate hyper;
extern crate tokio_timer;
extern crate web;

use futures::prelude::*;
use hyper::StatusCode;
use std::time::Duration;
use tokio_timer::sleep;
use web::*;

fn main() {
    let mut app = App::new();

    app.add(|_req, mut res: Response, _ctx, _next| {
        // Set a timeout that expires in 100 milliseconds
        let sleep = sleep(Duration::from_millis(100));

        sleep
            .map_err(|_| HttpError::Status(StatusCode::REQUEST_TIMEOUT))
            .and_then(move |_| res.body("Hello World!").map_err(HttpError::Http))
    });

    let app = app.build();
    let addr = ([127, 0, 0, 1], 3000).into();
    let server =
        hyper::Server::bind(&addr).serve(move || Ok::<_, ::std::io::Error>(app.serve(|| ())));
    println!("Listening on http://{}", server.local_addr());
    hyper::rt::run(server.map_err(|e| {
        eprintln!("Server Error: {}", e);
    }));
}
