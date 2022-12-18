use std::{env::var, fs, path::Path, process, sync::Arc};

use anyhow::{bail, Context, Error, Result};
use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use h3::{quic, server::RequestStream};
use http::{Method, Request, StatusCode};
use log::{debug, error, info, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    Config,
};
use quinn::{Connecting, Endpoint, ServerConfig};
use rustls_pemfile::Item::{ECKey, PKCS8Key, RSAKey};
use tokio::{fs::File, io::AsyncReadExt};

// Send buffer size
const SEND_SIZE: usize = 40960;
// Set ALPN protocols
const ALPN_QUIC_HTTP: &[&[u8]] = &[b"h3"];

#[tokio::main]
async fn main() {
    // Setup log file if set
    if let Some(logs) = var("LOGS").ok() {
        // Set log file
        let log_file = FileAppender::builder()
            .build(format!("{}/server.log", logs))
            .expect("failed to set log file");

        // Create logger config
        let config = Config::builder()
            .appender(Appender::builder().build("logfile", Box::new(log_file)))
            .build(Root::builder().appender("logfile").build(LevelFilter::Info))
            .expect("failed to create logger config");

        log4rs::init_config(config).expect("failed to create logger");
    }

    info!("Starting server...");

    // Check test case
    match var("TESTCASE").ok().as_deref() {
        Some("handshake") => {}
        Some(unknown) => {
            error!("unknown test case: {}", unknown);
            process::exit(127);
        }
        None => {
            error!("no test case set");
            process::exit(127);
        }
    }

    // Get paths if set
    let _qlogdir = var("QLOGDIR").ok();
    let www: Arc<Path> = var("WWW")
        .as_ref()
        .map(|path| Arc::from(Path::new(path)))
        .expect("www directory needs to be set");

    let config = create_config().expect("failed to create config");

    let (server, mut incoming) = Endpoint::server(
        config,
        format!(
            "{}:{}",
            var("IP").unwrap_or("[::1]".to_string()),
            var("PORT").unwrap_or("4433".to_string())
        )
        .parse()
        .expect("failed to parse address"),
    )
    .expect("failed to create connection endpoint");

    info!(
        "Starting to listen on {}.",
        server.local_addr().expect("failed to fetch local address")
    );

    // Handle new connections until the endpoint is closed
    while let Some(connection) = incoming.next().await {
        let handle = handle_connection(www.clone(), connection);

        tokio::spawn(async move {
            if let Err(why) = handle.await {
                error!("failed to handle connection: {}", why);
            }
        });
    }

    // Wait for connections to close
    server.wait_idle().await;
}

fn create_config() -> Result<ServerConfig> {
    // Get certificate file location
    let certs = var("CERTS").unwrap_or_default();

    // Read key and cert_chain
    let key = fs::read(format!("{}/priv.key", certs)).context("failed to read priv.key file")?;
    let cert_chain =
        fs::read(format!("{}/cert.pem", certs)).context("failed to read cert.pem file")?;

    // Parse key
    let key = match rustls_pemfile::read_one(&mut &*key).context("failed to parse pem file")? {
        Some(RSAKey(key)) => Ok::<_, Error>(key),
        Some(PKCS8Key(key)) => Ok(key),
        Some(ECKey(key)) => Ok(key),
        _ => bail!("couldn't find a key in the file"),
    }
    .map(rustls::PrivateKey)?;

    // Parse cert_chain
    let cert_chain = rustls_pemfile::certs(&mut &*cert_chain)
        .context("failed to parse cert.pem file")?
        .into_iter()
        .map(rustls::Certificate)
        .collect();

    // Create crypto config
    let mut crypto_config = rustls::ServerConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(&[&rustls::version::TLS13])
        .context("failed to set protocol version")?
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .context("invalid certificate/key")?;

    crypto_config.max_early_data_size = u32::MAX;
    crypto_config.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    // Set key log file
    crypto_config.key_log = Arc::new(rustls::KeyLogFile::new());

    // Create server config
    let config = ServerConfig::with_crypto(Arc::new(crypto_config));

    Ok(config)
}

async fn handle_connection(www: Arc<Path>, connection: Connecting) -> Result<()> {
    let connection = connection.await?;

    let mut h3_connection =
        h3::server::Connection::new(h3_quinn::Connection::new(connection)).await?;

    loop {
        let (req, stream) = match h3_connection.accept().await {
            Ok(Some((req, stream))) => (req, stream),
            Ok(None) => {
                info!("connection closed");
                return Ok(());
            }
            Err(why) => {
                bail!("stream closed due to unexpected error: {}", why);
            }
        };

        let handle = handle_request(www.clone(), req, stream);

        tokio::spawn(async move {
            if let Err(why) = handle.await {
                error!("failed to handle request: {}", why);
            }
        });
    }
}

async fn handle_request<T>(
    www: Arc<Path>,
    req: Request<()>,
    mut stream: RequestStream<T, Bytes>,
) -> Result<()>
where
    T: quic::BidiStream<Bytes>,
{
    debug!("Received request: {:?}", req);

    match *req.method() {
        Method::GET => {
            // Get path
            let path = www
                .to_path_buf()
                .join(req.uri().path().strip_prefix("/").unwrap_or_default());

            if !path.exists() {
                todo!("handle 404: {:?}", path);
            }

            let mut file = File::open(path).await.context("failed to open file")?;

            let response = http::Response::builder().status(StatusCode::OK).body(()).unwrap();

            stream.send_response(response).await?;

            loop {
                let mut buf = BytesMut::with_capacity(SEND_SIZE);

                if file
                    .read_buf(&mut buf)
                    .await
                    .context("failed to read the file")?
                    == 0
                {
                    break;
                }

                stream.send_data(buf.freeze()).await?;
            }

            stream.finish().await?;

            debug!("Responded to request successfully");
        }
        _ => {}
    }

    Ok(())
}
