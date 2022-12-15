use std::{env::var, fs, path::Path, sync::Arc, process};

use anyhow::{bail, Context, Error, Result};
use log::{error, info, debug, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    Config,
};
use quinn::{Connecting, Endpoint, ServerConfig, ConnectionError::ApplicationClosed, SendStream, RecvStream};
use rustls_pemfile::Item::{ECKey, PKCS8Key, RSAKey};
use tokio::{fs::File, io::AsyncReadExt};

// Set recv limit on socket to 8KiB
const RECV_LIMIT: usize = 8192;
// Set ALPN protocols
const ALPN_QUIC_HTTP: &[&[u8]] = &[b"h3", b"h3-32", b"h3-31", b"h3-30", b"h3-29", b"hq-interop", b"hq-32", b"hq-31", b"hq-30", b"hq-29", b"siduck"];

#[tokio::main]
async fn main() {
    // Setup log file if set
    if let Some(logs) = var("LOGS").ok() {
        // Set log file
        let log_file = FileAppender::builder().build(format!("{}/server.log", logs)).expect("failed to set log file");

        // Create logger config
        let config = Config::builder()
            .appender(Appender::builder().build("logfile", Box::new(log_file)))
            .build(Root::builder().appender("logfile").build(LevelFilter::Info)).expect("failed to create logger config");

        log4rs::init_config(config).expect("failed to create logger");
    }

    info!("Starting server...");

    // Check test case
    match var("TESTCASE").ok().as_deref() {
        Some("handshake") => {
        }
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

    let server = Endpoint::server(
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
    while let Some(connection) = server.accept().await {
        let handle = handle_connection(www.clone(), connection);

        tokio::spawn(async move {
            if let Err(why) = handle.await {
                error!("failed to handle connection: {}", why);
            }
        });
    }
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
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .context("invalid certificate/key")?;

    crypto_config.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    // Set key log file
    crypto_config.key_log = Arc::new(rustls::KeyLogFile::new());

    // Create server config
    let config = ServerConfig::with_crypto(Arc::new(crypto_config));

    Ok(config)
}

async fn handle_connection(www: Arc<Path>, connection: Connecting) -> Result<()> {
    let connection = connection.await?;

    loop {
        let stream = match connection.accept_bi().await {
            Ok(stream) => stream,
            Err(ApplicationClosed { .. }) => {
                info!("connection closed");
                return Ok(());
            },
            Err(why) => {
                bail!("connection closed due to unexpected error: {}", why);
            }
        };

        let handle = handle_request(www.clone(), stream);

        tokio::spawn(async move {
            if let Err(why) = handle.await {
                error!("failed to handle request: {}", why);
            }
        });
    }
}

async fn handle_request(www: Arc<Path>, (mut send, recv): (SendStream, RecvStream)) -> Result<()> {
    let request = recv.read_to_end(RECV_LIMIT).await?;

    // Parse request
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut req = httparse::Request::new(&mut headers);
    let res = req.parse(&request)?;

    debug!("Received request: {:?}", req);

    // Get path
    if res.is_partial() {
        if let Some("GET") = req.method {
            if let Some(path) = req.path {
                // Get path
                let path = www.to_path_buf().join(Path::new(path));

                if !path.exists() {
                    todo!("add 404 handling");
                }

                let mut file = File::open(path).await?;
                let mut buf = Vec::new();

                file.read_to_end(&mut buf).await?;

                send.write_all(&buf).await?;
                send.finish().await?;

                debug!("Responded to request successfully");
            }
        }
    }

    Ok(())
}
