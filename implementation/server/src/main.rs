use std::{env::var, fs, path::Path, process, sync::Arc};

use anyhow::{bail, Context, Error, Result};
use bytes::{Bytes, BytesMut};
use derive_builder::Builder;
use futures::StreamExt;
use h3::{quic, server::RequestStream};
use http::{Method, Request, StatusCode};
use log::{debug, error, info, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    Config,
};
use quinn::{Connecting, Endpoint, EndpointConfig, ServerConfig, TransportConfig};
use rustls_pemfile::Item::{ECKey, PKCS8Key, RSAKey};
use tokio::{fs::File, io::AsyncReadExt};

// Send buffer size
const SEND_SIZE: usize = 40960;
// Set ALPN protocols
const ALPN_QUIC_HTTP: &[&[u8]] = &[b"h3"];
// Supported versions for version negotiation
const RESTRICTED_SUPPORTED_VERSIONS: &[u32] = &[0x00000001];

#[derive(Builder, Default)]
#[builder(default)]
struct Options {
    // Whether to restrict the supported versions
    restrict_versions: bool,
    // Whether to generate a Retry
    retry: bool,
    // Whether to use TLS_CHACHA20_POLY1305_SHA256 only as a cipher suite
    chacha_only: bool,
    // The number of maximum streams bidi streams, open at the same time
    #[builder(setter(strip_option))]
    max_streams: Option<u32>,
    // The log level of the application (defaults to info)
    #[builder(setter(strip_option))]
    log_level: Option<LevelFilter>,
}

#[tokio::main]
async fn main() {
    // Check test case
    let options = match var("TESTCASE").ok().as_deref() {
        Some("handshake") => OptionsBuilder::default().build(),
        Some("transfer") => OptionsBuilder::default().build(),
        Some("multihandshake") => OptionsBuilder::default().build(),
        Some("versionnegotiation") => OptionsBuilder::default().restrict_versions(true).build(),
        Some("chacha20") => OptionsBuilder::default().chacha_only(true).build(),
        Some("retry") => OptionsBuilder::default().retry(true).build(),
        Some("resumption") => OptionsBuilder::default().build(),
        Some("zerortt") => OptionsBuilder::default().build(),
        Some("transportparameter") => OptionsBuilder::default().max_streams(10).build(),
        Some("goodput") => OptionsBuilder::default()
            .log_level(LevelFilter::Off)
            .build(),
        Some("optimize") => OptionsBuilder::default()
            .max_streams(255)
            .log_level(LevelFilter::Off)
            .build(),
        Some(unknown) => {
            error!("unknown test case: {}", unknown);
            process::exit(127);
        }
        None => {
            error!("no test case set");
            process::exit(127);
        }
    }
    .expect("failed to build options");

    // Setup log file if set
    if let Some(logs) = var("LOGS").ok() {
        // Set log file
        let log_file = FileAppender::builder()
            .build(format!("{}/server.log", logs))
            .expect("failed to set log file");

        // Create logger config
        let config = Config::builder()
            .appender(Appender::builder().build("logfile", Box::new(log_file)))
            .build(
                Root::builder()
                    .appender("logfile")
                    .build(options.log_level.unwrap_or(LevelFilter::Info)),
            )
            .expect("failed to create logger config");

        log4rs::init_config(config).expect("failed to create logger");
    }

    info!("Starting server...");

    // Get paths if set
    let _qlogdir = var("QLOGDIR").ok();
    let www: Arc<Path> = var("WWW")
        .as_ref()
        .map(|path| Arc::from(Path::new(path)))
        .expect("www directory needs to be set");

    let config = create_config(&options).expect("failed to create config");

    let (server, mut incoming) = {
        let addr: std::net::SocketAddr = format!(
            "{}:{}",
            var("IP").unwrap_or("[::1]".to_string()),
            var("PORT").unwrap_or("4433".to_string())
        )
        .parse()
        .expect("failed to parse address");

        let mut endpoint_config = EndpointConfig::default();

        if options.restrict_versions {
            endpoint_config.supported_versions(RESTRICTED_SUPPORTED_VERSIONS.to_vec());
        }

        let socket = std::net::UdpSocket::bind(addr).expect("failed to open udp socket");
        Endpoint::new(endpoint_config, Some(config), socket)
            .expect("failed to create connection endpoint")
    };

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

fn create_config(options: &Options) -> Result<ServerConfig> {
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
    let crypto_config = rustls::ServerConfig::builder();

    let crypto_config = if options.chacha_only {
        crypto_config.with_cipher_suites(&[rustls::cipher_suite::TLS13_CHACHA20_POLY1305_SHA256])
    } else {
        crypto_config.with_safe_default_cipher_suites()
    };

    let mut crypto_config = crypto_config
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

    // Create transport config
    let mut transport_config = TransportConfig::default();

    if let Some(max_streams) = options.max_streams {
        transport_config.max_concurrent_bidi_streams(max_streams.into());
    }

    // Create server config
    let mut config = ServerConfig::with_crypto(Arc::new(crypto_config));
    config.transport = Arc::new(transport_config);
    config.use_retry(options.retry);

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

            let response = http::Response::builder()
                .status(StatusCode::OK)
                .body(())
                .unwrap();

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
