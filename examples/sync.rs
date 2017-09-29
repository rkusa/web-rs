extern crate ctx;
extern crate futures_cpupool;
extern crate hyper;
extern crate web;

use ctx::{background, with_value};
use hyper::server::{Http, Response};
use std::thread;
use std::time::Duration;
use web::*;
use futures_cpupool::CpuPool;

fn main() {
    let pool = CpuPool::new(32);
    let mut app = App::new();

    app.add(|_req, mut res: Response, ctx: Context, _next| {
        let pool = ctx.value::<CpuPool>().unwrap();

        pool.spawn_fn(|| {
            thread::sleep(Duration::from_millis(1000));

            res.set_body("Hello World!");
            Ok(res)
        })
    });

    let app = app.build();
    let addr = "127.0.0.1:3000".parse().unwrap();
    let server = Http::new()
        .bind(&addr, move || {
            let pool = pool.clone();
            Ok(app.handle(move || with_value(background(), pool.clone())))
        })
        .unwrap();
    println!(
        "Listening on http://{} with 1 thread.",
        server.local_addr().unwrap()
    );
    server.run().unwrap();
}
