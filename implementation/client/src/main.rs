use std::{env::var, net::ToSocketAddrs, path::Path, process, sync::Arc};

use anyhow::{Context, Result};
use bytes::{Buf, Bytes};
use futures::future::{self, join_all};
use h3::{
    client::{Connection, SendRequest},
    quic,
};
use log::{debug, error, info, warn, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    Config,
};
use quinn::{ClientConfig, Connecting, Endpoint};
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;

// Set ALPN protocols
const ALPN_QUIC_HTTP: &[&[u8]] = &[b"h3"];

#[tokio::main]
async fn main() {
    // Setup log file if set
    if let Some(logs) = var("LOGS").ok() {
        // Set log file
        let log_file = FileAppender::builder()
            .build(format!("{}/client.log", logs))
            .expect("failed to set log file");

        // Create logger config
        let config = Config::builder()
            .appender(Appender::builder().build("logfile", Box::new(log_file)))
            .build(Root::builder().appender("logfile").build(LevelFilter::Info))
            .expect("failed to create logger config");

        log4rs::init_config(config).expect("failed to create logger");
    }

    info!("Starting client...");

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
    let downloads: Arc<Path> = var("DOWNLOADS")
        .as_ref()
        .map(|path| Arc::from(Path::new(path)))
        .expect("downloads directory needs to be set");

    let config = create_config().expect("failed to create config");

    let mut client =
        Endpoint::client("[::]:0".parse().unwrap()).expect("failed to create connection endpoint");

    client.set_default_client_config(config);

    // Load request addresses
    let requests = var("REQUESTS").unwrap_or_default();
    let requests = requests
        .split_whitespace()
        .filter_map(|url| Url::parse(url).ok());

    let mut handles = Vec::new();

    for url in requests {
        // Get connection address
        let host_str = url.host_str().expect("host string not set");
        let remote = (host_str, url.port().unwrap_or(4433))
            .to_socket_addrs()
            .expect("failed to parse addresses")
            .next()
            .expect("invalid request address");

        info!("Connecting to {}", url);

        // Create connection
        let connection = client
            .connect(remote, host_str)
            .expect("failed to create connection");

        let handle = connect(downloads.clone(), url, connection);

        // Connect to the server
        handles.push(tokio::spawn(async move {
            if let Err(why) = handle.await {
                error!("failed to connect to the server: {}", why);
            }
        }));
    }

    join_all(handles).await;

    client.wait_idle().await;
}

fn create_config() -> Result<ClientConfig> {
    // Create root certificate
    let mut roots = rustls::RootCertStore::empty();

    for cert in rustls_native_certs::load_native_certs().context("failed to load platform certs")? {
        if let Err(why) = roots.add(&rustls::Certificate(cert.0)) {
            warn!("failed to parse trust anchor: {}", why);
        }
    }

    // Create crypto config
    let mut crypto_config = rustls::ClientConfig::builder()
        .with_safe_default_cipher_suites()
        .with_safe_default_kx_groups()
        .with_protocol_versions(&[&rustls::version::TLS13])
        .context("failed to set protocol version")?
        .with_root_certificates(roots)
        .with_no_client_auth();

    crypto_config.enable_early_data = true;
    crypto_config.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    // Set key log file
    crypto_config.key_log = Arc::new(rustls::KeyLogFile::new());

    // Create client config
    let config = ClientConfig::new(Arc::new(crypto_config));

    Ok(config)
}

async fn connect(downloads: Arc<Path>, url: Url, connection: Connecting) -> Result<()> {
    let connection = connection.await?;

    let (driver, send) = h3::client::new(h3_quinn::Connection::new(connection)).await?;

    let handle = handle_request(downloads, url, send);
    let drive = drive_request(driver);

    let (handle_res, drive_res) = tokio::join!(handle, drive);
    handle_res?;
    drive_res?;

    Ok(())
}

async fn drive_request<T>(mut driver: Connection<T, Bytes>) -> Result<()>
where
    T: quic::Connection<Bytes>,
{
    future::poll_fn(|cx| driver.poll_close(cx)).await?;
    Ok(())
}

async fn handle_request<T>(
    downloads: Arc<Path>,
    url: Url,
    mut send: SendRequest<T, Bytes>,
) -> Result<()>
where
    T: quic::OpenStreams<Bytes>,
{
    debug!("Sending a request to {}", url);

    let req = http::Request::builder().uri(url.as_str()).body(())?;

    let mut stream = send.send_request(req).await?;

    // finish on the sending side
    stream.finish().await?;

    debug!("Receiving a response...");

    let resp = stream.recv_response().await?;

    debug!("Response: {:?} {}", resp.version(), resp.status());

    let file_name = Path::new(url.path()).file_name().unwrap_or_default();
    let path = downloads.to_path_buf().join(file_name);

    let mut file = File::create(path).await?;

    while let Some(buf) = stream.recv_data().await? {
        file.write_all(buf.chunk()).await?;
    }

    Ok(())
}
