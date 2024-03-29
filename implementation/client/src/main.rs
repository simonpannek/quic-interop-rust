use std::{env::var, net::ToSocketAddrs, path::Path, process, sync::Arc};

use anyhow::{anyhow, Context, Result};
use bytes::{Buf, Bytes};
use derive_builder::Builder;
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

#[derive(Builder, Clone, Default)]
#[builder(default)]
struct Options {
    // Whether to download all files using a single connection
    single_connection: bool,
    // Set the version number of QUIC
    #[builder(setter(strip_option))]
    version: Option<u32>,
    // Whether to use TLS_CHACHA20_POLY1305_SHA256 only as a cipher suite
    chacha_only: bool,
    // Whether to download the first file using a separate connection
    first_separate: bool,
    // Whether to use 0-RTT after reconnecting to a server
    zero_rtt: bool,
    // The log level of the application (defaults to info)
    #[builder(setter(strip_option))]
    log_level: Option<LevelFilter>,
}

struct Verifier;

impl Verifier {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

#[tokio::main]
async fn main() {
    // Check test case
    let options = match var("TESTCASE").ok().as_deref() {
        Some("handshake") => OptionsBuilder::default().build(),
        Some("transfer") => OptionsBuilder::default().single_connection(true).build(),
        Some("multihandshake") => OptionsBuilder::default().build(),
        Some("versionnegotiation") => OptionsBuilder::default().version(0xff00001d).build(),
        Some("chacha20") => OptionsBuilder::default().chacha_only(true).build(),
        Some("retry") => OptionsBuilder::default().build(),
        Some("resumption") => OptionsBuilder::default()
            .single_connection(true)
            .first_separate(true)
            .build(),
        Some("zerortt") => OptionsBuilder::default()
            .single_connection(true)
            .first_separate(true)
            .zero_rtt(true)
            .build(),
        Some("transportparameter") => OptionsBuilder::default().single_connection(true).build(),
        Some("goodput") => OptionsBuilder::default()
            .log_level(LevelFilter::Off)
            .build(),
        Some("optimize") => OptionsBuilder::default()
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
            .build(format!("{}/client.log", logs))
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

    info!("Starting client...");

    // Get paths if set
    let _qlogdir = var("QLOGDIR").ok();
    let downloads: Arc<Path> = var("DOWNLOADS")
        .as_ref()
        .map(|path| Arc::from(Path::new(path)))
        .expect("downloads directory needs to be set");

    let config = create_config(&options).expect("failed to create config");

    let mut client =
        Endpoint::client("[::]:0".parse().unwrap()).expect("failed to create connection endpoint");

    client.set_default_client_config(config);

    // Load request addresses
    let requests = var("REQUESTS").unwrap_or_default();
    let requests: Vec<_> = requests
        .split_whitespace()
        .filter_map(|url| Url::parse(url).ok())
        .collect();

    let mut handles = Vec::new();

    let slice_size = if options.single_connection {
        requests.len()
    } else {
        1
    };

    for i in 0..requests.len() / slice_size {
        let i = i * slice_size;
        let mut urls = &requests[i..i + slice_size];

        // Get connection address
        let url = urls.get(0).unwrap();
        let host_str = url.host_str().expect("host string not set");
        let remote = (host_str, url.port().unwrap_or(4433))
            .to_socket_addrs()
            .expect("failed to parse addresses")
            .next()
            .expect("invalid request address");

        info!("Connecting to {}", url);

        if i == 0 && options.first_separate {
            // Create connection
            let connection = client
                .connect(remote, host_str)
                .expect("failed to create connection");

            connect(false, downloads.clone(), vec![url.clone()], connection)
                .await
                .expect("failed to connect to the server");

            urls = &urls[1..];
        }

        // Create connection
        match client.connect(remote, host_str) {
            Ok(connection) => {
                let handle = connect(
                    options.zero_rtt,
                    downloads.clone(),
                    Vec::from(urls),
                    connection,
                );

                // Connect to the server
                handles.push(tokio::spawn(async move {
                    if let Err(why) = handle.await {
                        error!("failed to connect to the server: {}", why);
                    }
                }));
            }
            Err(why) => error!("failed to create connection: {}", why),
        }
    }

    join_all(handles).await;

    client.wait_idle().await;
}

fn create_config(options: &Options) -> Result<ClientConfig> {
    // Create root certificate
    let mut roots = rustls::RootCertStore::empty();

    for cert in rustls_native_certs::load_native_certs().context("failed to load platform certs")? {
        if let Err(why) = roots.add(&rustls::Certificate(cert.0)) {
            warn!("failed to parse trust anchor: {}", why);
        }
    }

    // Create crypto config
    let crypto_config = rustls::ClientConfig::builder();

    let crypto_config = if options.chacha_only {
        crypto_config.with_cipher_suites(&[rustls::cipher_suite::TLS13_CHACHA20_POLY1305_SHA256])
    } else {
        crypto_config.with_safe_default_cipher_suites()
    };

    let mut crypto_config = crypto_config
        .with_safe_default_kx_groups()
        .with_protocol_versions(&[&rustls::version::TLS13])
        .context("failed to set protocol version")?
        .with_custom_certificate_verifier(Verifier::new())
        .with_no_client_auth();

    crypto_config.enable_early_data = true;
    crypto_config.alpn_protocols = ALPN_QUIC_HTTP.iter().map(|&x| x.into()).collect();

    // Set key log file
    crypto_config.key_log = Arc::new(rustls::KeyLogFile::new());

    // Create client config
    let mut config = ClientConfig::new(Arc::new(crypto_config));

    if let Some(version) = options.version {
        config.version(version);
    }

    Ok(config)
}

async fn connect(
    zero_rtt: bool,
    downloads: Arc<Path>,
    urls: Vec<Url>,
    connection: Connecting,
) -> Result<()> {
    let connection = if zero_rtt {
        connection
            .into_0rtt()
            .map(|x| x.0)
            .map_err(|why| anyhow!("failed to connect using 0-RTT: {:?}", why))?
    } else {
        connection.await?
    };

    let (driver, send) = h3::client::new(h3_quinn::Connection::new(connection)).await?;

    let handle = handle_request(downloads, urls, send);
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
    urls: Vec<Url>,
    mut send: SendRequest<T, Bytes>,
) -> Result<()>
where
    T: quic::OpenStreams<Bytes>,
{
    for url in urls {
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
    }

    Ok(())
}
