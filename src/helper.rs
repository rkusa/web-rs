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

    Box::new(move |mut req| {
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

            resolve_result(mw(req), uri_before)
        } else {
            Next(req)
        }
    })
}

fn resolve_result(result: Respond, uri_before: Uri) -> Respond {
    match result {
        Next(mut req) => {
            req.set_uri(uri_before);
            Next(req)
        }
        Done(mut req, res) => {
            req.set_uri(uri_before);
            Done(req, res)
        }
        Async(fut) => fut.map(|r| resolve_result(r, uri_before)).into(),
    }
}

#[cfg(test)]
mod tests {
    use App;
    use Respond::*;

    #[test]
    fn combine() {
        let mut app = App::new();
        app.attach(combine!(|req| Next(req), |req| Next(req)));
    }
}