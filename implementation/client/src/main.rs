use std::{env::var, net::ToSocketAddrs, path::Path, process, sync::Arc};

use anyhow::{Result};
use log::{error, info, LevelFilter};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    Config,
};
use quinn::{ClientConfig, Endpoint, Connection};
use url::Url;

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
    let _downloads: Arc<Path> = var("DOWNLOADS")
        .as_ref()
        .map(|path| Arc::from(Path::new(path)))
        .expect("www directory needs to be set");

    let config = create_config();

    let mut client =
        Endpoint::client("[::]:0".parse().unwrap()).expect("failed to create connection endpoint");

    client.set_default_client_config(config);

    // Load request addresses
    let requests = var("REQUESTS").unwrap_or_default();
    let requests = requests
        .split_whitespace()
        .filter_map(|url| Url::parse(url).ok());

    for url in requests {
        // Get connection address
        let host_str = url.host_str().expect("host string not set");
        let remote = (
            host_str,
            url.port().unwrap_or(4433),
        )
            .to_socket_addrs()
            .expect("failed to parse addresses")
            .next()
            .expect("invalid request address");

        info!("Connecting to {}", url);

        // Create connection
        let connection = client
            .connect(remote, host_str)
            .expect("failed to create connection");

        // Connect to the server
        tokio::spawn(async move {
            match connection.await {
                Ok(connection) => {
                    if let Err(why) = connect(url, connection).await {
                        error!("failure after connecting to the server: {}", why);
                    }
                },
                Err(why) => {
                    error!("failure connecting to the server: {}", why);
                }
            }
        });
    }
}

fn create_config() -> ClientConfig {
    // Create crypto config
    let mut crypto_config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();

    // Set key log file
    crypto_config.key_log = Arc::new(rustls::KeyLogFile::new());

    // Return client
    ClientConfig::new(Arc::new(crypto_config))
}

async fn connect(_url: Url, _connection: Connection) -> Result<()> {
    Ok(())
}
