use std::str::FromStr;

use futures::Future;
use hyper::Uri;
use {Middleware, Respond};
use Respond::*;

#[macro_export]
macro_rules! combine {
    ( $( $x:expr ),* ) => {
        {
            let mut app = App::new();
            $(
                app.attach($x);
            )*
            app.middleware()
        }
    };
}

pub fn mount(path: &str, mw: Middleware) -> Middleware {
    let path = path.to_owned();

    Box::new(move |mut req, res, ctx| {
        if req.uri().path().starts_with(path.as_str()) {
            let uri_before = req.uri().clone();

            // TODO: extend hyper to not have to create a URI from string
            let new_uri = {
                let mut s = String::from("");
                if let Some(scheme) = uri_before.scheme() {
                    s += scheme;
                    s += "://";
                }
                if let Some(authority) = uri_before.authority() {
                    s += authority;
                }

                let (_, mut new_path) = uri_before.path().split_at(path.len());
                if new_path.len() == 0 {
                    new_path = "/";
                }
                s += new_path;

                if let Some(query) = uri_before.query() {
                    s += "?";
                    s += query;
                }
                // TODO: does not contain fragment (because currently not exposed by hyper)
                Uri::from_str(s.as_str()).unwrap()
            };

            req.set_uri(new_uri);

            resolve_result(mw(req, res, ctx), uri_before)
        } else {
            Next(req, res, ctx)
        }
    })
}

fn resolve_result(result: Respond, uri_before: Uri) -> Respond {
    match result {
        Next(mut req, res, ctx) => {
            req.set_uri(uri_before);
            Next(req, res, ctx)
        }
        Done(res) => Done(res),
        Async(fut) => fut.map(|r| resolve_result(r, uri_before)).into(),
        Throw(err) => Throw(err),
    }
}

#[cfg(test)]
mod tests {
    use App;
    use Respond::*;

    #[test]
    fn combine() {
        let mut app = App::new();
        app.attach(combine!(|req, res, ctx| Next(req, res, ctx),
                            |req, res, ctx| Next(req, res, ctx)));
    }
}
