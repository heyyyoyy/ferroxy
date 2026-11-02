use std::convert::Infallible;
use std::net::SocketAddr;

use hyper::{Request, Response, body::Bytes, server::conn::http1, service::service_fn};
use tokio::net::TcpListener;

use http_body_util::Full;
use hyper_util::rt::{TokioIo, TokioTimer};
use tracing::{Instrument, debug, error, info, info_span};
use tracing_subscriber::EnvFilter;

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_tracing();

    let addr: SocketAddr = ([127, 0, 0, 1], 3000).into();
    let listener = TcpListener::bind(addr).await?;
    info!(listen_addr = %addr, "listener started on http://{}", addr);
    loop {
        let (socket, peer) = listener.accept().await?;
        let io = TokioIo::new(socket);
        debug!(%peer, "accepted tcp connection");
        tokio::task::spawn(
            async move {
                if let Err(err) = http1::Builder::new()
                    .timer(TokioTimer::new())
                    .serve_connection(
                        io,
                        service_fn(|req: Request<hyper::body::Incoming>| async move {
                            debug!(
                                method = %req.method(),
                                path = %req.uri().path(),
                                "request"
                            );
                            Ok::<Response<Full<Bytes>>, Infallible>(Response::new(Full::new(
                                Bytes::from("Hello"),
                            )))
                        }),
                    )
                    .await
                {
                    error!(error = %err, "connection failed");
                }
            }
            .instrument(info_span!("connection", peer = %peer)),
        );
    }
}
