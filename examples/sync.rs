extern crate futures;
extern crate futures_cpupool;
extern crate hyper;
extern crate web;

use futures::Future;
use futures_cpupool::CpuPool;
use std::thread;
use std::time::Duration;
use web::*;

fn main() {
    let pool = CpuPool::new(32);
    let mut app = App::new();

    app.add(|_req, mut res: Response, pool: CpuPool, _next| {
        pool.spawn_fn(move || {
            thread::sleep(Duration::from_millis(1000));

            res.body("Hello World!").into_response()
        })
    });

    let app = app.build();
    let addr = "127.0.0.1:3000".parse().unwrap();
    let server = hyper::Server::bind(&addr).serve(move || {
        let pool = pool.clone();
        Ok::<_, ::std::io::Error>(app.serve(move || pool.clone()))
    });
    println!("Listening on http://{}", server.local_addr());
    hyper::rt::run(server.map_err(|e| {
        eprintln!("Server Error: {}", e);
    }));
}
