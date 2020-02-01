use crate::prelude::*;
use crate::AsyncRuntime;
use crate::Body;
use crate::Error;
use async_std_lib::sync::channel;
use futures_util::future::FutureExt;
use futures_util::select;
use std::net;
use tide;

mod basic;
mod simplelog;

pub fn test_setup() {
    simplelog::set_logger();
    // We're using async-std for the tests because that's what tide uses.
    AsyncRuntime::set_default(AsyncRuntime::AsyncStd);
}

#[allow(clippy::type_complexity)]
pub fn run_server<Res>(
    req: http::Request<Body>,
    res: Res,
) -> Result<(http::Request<()>, http::Response<()>, Vec<u8>), Error>
where
    Res: tide::IntoResponse + Clone + Sync + 'static,
{
    test_setup();
    AsyncRuntime::current().block_on(async {
        // channel where we "leak" the server request from tide
        let (txsreq, rxsreq) = channel(1);
        // channel where we shut down tide server using select! macro
        let (txend, rxend) = channel::<()>(1);

        let mut app = tide::new();
        app.at("/*path").all(move |req| {
            let txsreq = txsreq.clone();
            let resp = res.clone();
            async move {
                txsreq.send(req).await;
                resp
            }
        });

        let (hostport, test_uri) = random_test_uri(req.uri());

        // Rewrite the incoming request to use the port.
        let (mut parts, body) = req.into_parts();
        parts.uri = test_uri;
        let req = http::Request::from_parts(parts, body);

        // Run the server app in a select! that ends when we send the end signal.
        AsyncRuntime::current().spawn(async move {
            select! {
                a = app.listen(&hostport).fuse() => a.map_err(|e| Error::Io(e)),
                b = rxend.recv().fuse() => Ok(()),
            }
        });

        // Send request and wait for the client response.
        let client_res = req.send().await?;

        // Normalize client response
        let (parts, mut body) = client_res.into_parts();
        // Read out entire response bytes to a vec.
        let client_bytes = body.read_to_vec().await?;
        let client_res = http::Response::from_parts(parts, ());

        // Wait for the server request to "leak out" of the server app.
        let tide_server_req = rxsreq.recv().await.expect("Wait for server request");

        let server_req = normalize_tide_request(tide_server_req);

        // Shut down the server.
        txend.send(()).await;

        Ok((server_req, client_res, client_bytes))
    })
}

/// Generate a random hos:port and uri pair
fn random_test_uri(uri: &http::Uri) -> (String, http::Uri) {
    // TODO There's no guarantee this port will be free by the time we do app.listen()
    // this could lead to random test failures. If tide provided some way of binding :0
    // and returning the port bound would be the best.
    let port = random_local_port();
    let hostport = format!("127.0.0.1:{}", port);
    let pq = uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .expect("Test bad pq");

    // This is the request uri for the test
    let test_uri_s = format!("http://{}{}", hostport, pq);
    let test_uri = test_uri_s
        .as_str()
        .parse::<http::Uri>()
        .expect("Test bad req uri");

    (hostport, test_uri)
}

/// there's no guarantee this port wil be available when we want to (re-)use it
fn random_local_port() -> u16 {
    let mut n = 0;
    loop {
        n += 1;
        if n > 100 {
            panic!("Failed to allocate port after 100 retries");
        }
        let socket = net::SocketAddrV4::new(net::Ipv4Addr::LOCALHOST, 0);
        let port = net::TcpListener::bind(socket)
            .and_then(|listener| listener.local_addr())
            .and_then(|addr| Ok(addr.port()))
            .ok();
        if let Some(port) = port {
            break port;
        }
    }
}

/// Normalize tide request to a http::Request<()>
fn normalize_tide_request(tide_req: tide::Request<()>) -> http::Request<()> {
    let pq = tide_req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .expect("Test bad tide pq");
    let mut server_req = http::Request::builder()
        .method(tide_req.method().as_str())
        .uri(pq);
    for (k, v) in tide_req.headers().clone().into_iter() {
        if let (Some(k), Some(v)) = (k, v.to_str().ok()) {
            server_req = server_req.header(k.as_str(), v);
        }
    }
    server_req.body(()).expect("Normalize tide req")
}

#[macro_export]
macro_rules! test_h1_h2 {
    (fn $name:ident () -> $ret:ty { $($body:tt)* } $($rest:tt)*) => {
        paste::item! {
            #[test]
            fn [<$name _h1>]() -> $ret {
                let bld = http::Request::builder();
                let close = $($body)*;
                (close)(bld)
            }
            #[test]
            fn [<$name _h2>]() -> $ret {
                let bld = http::Request::builder().force_http2(true);
                let close = $($body)*;
                (close)(bld)
            }
        }
        test_h1_h2!($($rest)*);
    };
    () => ()
}
