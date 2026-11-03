use std::process::ExitCode;
use std::{convert::Infallible, path::PathBuf};

use clap::Parser;
use hyper::{Request, Response, body::Bytes, server::conn::http1, service::service_fn};
use tokio::net::TcpListener;

use http_body_util::Full;
use hyper_util::rt::{TokioIo, TokioTimer};
use tracing::{Instrument, debug, error, info, info_span};
use tracing_subscriber::EnvFilter;

#[derive(Debug, clap::Parser)]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Path to the configuration file
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,
    /// Validate the configuration and exit
    #[arg(long)]
    check: bool,
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.check {
        let cfg = ferroxy::config::Config::load(&cli.config)?;
        eprintln!("config ok: {} routes", cfg.routes.len());
        return Ok(());
    }
    init_tracing();
    let cfg = ferroxy::config::Config::load(&cli.config)?;
    let listener = TcpListener::bind(cfg.server.https_listen).await?;
    info!(listen_addr = %cfg.server.https_listen, "listener started on http://{}", cfg.server.https_listen);
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
