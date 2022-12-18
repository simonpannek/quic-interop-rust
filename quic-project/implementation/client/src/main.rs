use std::{env::var, net::ToSocketAddrs, path::Path, process, sync::Arc};

use anyhow::{Context, Result};
use futures::future::join_all;
use log::{error, info, warn, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    Config,
};
use quinn::{ClientConfig, Connecting, Endpoint};
use tokio::{fs::File, io::AsyncWriteExt};
use url::Url;

// Set ALPN protocols
const ALPN_QUIC_HTTP: &[&[u8]] = &[
    b"h3",
    b"h3-32",
    b"h3-31",
    b"h3-30",
    b"h3-29",
    b"hq-interop",
    b"hq-32",
    b"hq-31",
    b"hq-30",
    b"hq-29",
    b"siduck",
];

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
        .with_safe_defaults()
        .with_root_certificates(roots.clone())
        .with_no_client_auth();

    crypto_config.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    // Set key log file
    crypto_config.key_log = Arc::new(rustls::KeyLogFile::new());

    // Create client config
    let config = ClientConfig::new(Arc::new(crypto_config));
    let config = ClientConfig::with_root_certificates(roots);

    Ok(config)
}

async fn connect(downloads: Arc<Path>, url: Url, connection: Connecting) -> Result<()> {
    let connection = connection.await?;

    /*let (mut send, recv) = connection.open_bi().await?;

    // Send request
    let request = format!("GET {}\r\n", url.path());
    send.write_all(request.as_bytes()).await?;
    send.finish().await?;

    // Get response
    let response = recv.read_to_end(usize::max_value()).await?;

    let file_name = Path::new(url.path()).file_name().unwrap_or_default();
    let path = downloads.to_path_buf().join(file_name);

    // Write response to file
    let mut file = File::create(path).await?;
    file.write_all(&response).await?;*/

    Ok(())
}
