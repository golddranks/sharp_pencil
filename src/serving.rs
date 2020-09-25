//! This module implements the http server support for our application.

use std::{convert::Infallible, convert::TryFrom, net::ToSocketAddrs, sync::Arc};

use hyper::{Body, Request, Response, StatusCode, server::Server, server::conn::AddrStream, service::make_service_fn, service::service_fn};

use crate::app::Pencil;

/// Run the `Pencil` application.
pub async fn run_server<A: ToSocketAddrs>(app: Pencil, addr: A) {

    let app = Arc::new(app);

    let make_svc = make_service_fn(move |_: &AddrStream| {
        let app = app.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |request: Request<Body>| {
                let app = app.clone();
                async move {
                    Ok::<_, Infallible>({
                        debug!("Request: {}", request.uri());
                        let app = app.clone();
                        let mut res = Response::builder();
                        let (body, status) = match crate::wrappers::Request::new(&*app, request) {
                            Ok(mut request) => {
                                let response = app.handle_request(&mut request);
                                *res.headers_mut().unwrap() = response.headers;
                                (response.body.unwrap_or_default(), StatusCode::try_from(response.status_code).expect("TODO"))
                            }
                            Err(_) => {
                                (Body::empty(), StatusCode::BAD_REQUEST)
                            }
                        };
                        res.status(status).body(body).unwrap()
                    })
                }
            }))
        }
    });


    let addr = addr.to_socket_addrs().expect("TODO").next().expect("TODO");
    Server::bind(&addr).serve(make_svc).await.unwrap();
}
