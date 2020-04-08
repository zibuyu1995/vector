use crate::{
    runtime::Runtime,
    topology::config::{SinkConfig, SinkContext},
};
use futures01::{sync::mpsc, Future, Sink, Stream};
use hyper::{service::service_fn, Body, Request, Response, Server};
use serde::Deserialize;

pub fn load_sink<T>(config: &str) -> crate::Result<(T, SinkContext, Runtime)>
where
    for<'a> T: Deserialize<'a> + SinkConfig,
{
    let sink_config: T = toml::from_str(config)?;
    let rt = crate::test_util::runtime();
    let cx = SinkContext::new_test(rt.executor());

    Ok((sink_config, cx, rt))
}

pub fn build_test_server(
    addr: &std::net::SocketAddr,
) -> (
    mpsc::Receiver<(http::request::Parts, bytes05::Bytes)>,
    stream_cancel::Trigger,
    impl Future<Item = (), Error = ()>,
) {
    let (tx, rx) = mpsc::channel(100);
    let service = hyper::service::make_service_fn(move |_| {
        let tx = tx.clone();
        let svc = service_fn(move |req: Request<Body>| {
            let (parts, body) = req.into_parts();

            let tx = tx.clone();

            tokio::spawn(async move {
                let body = hyper::body::aggregate(body).await.unwrap();
                use bytes05::Buf;
                tx.send((parts, body.to_bytes()));
            });

            let res = Response::new(Body::empty());
            futures::future::ok::<_, std::convert::Infallible>(res)
        });

        futures::future::ok::<_, std::convert::Infallible>(svc)
    });

    let (trigger, tripwire) = stream_cancel::Tripwire::new();

    use futures::compat::Future01CompatExt;
    use futures::{FutureExt, TryFutureExt};

    let server = Server::bind(addr)
        .serve(service)
        .with_graceful_shutdown(tripwire.compat().map(drop))
        .map_err(|e| panic!("server error: {}", e))
        .boxed()
        .compat();

    (rx, trigger, server)
}
