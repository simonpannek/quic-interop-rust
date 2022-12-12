use std::{env::var, fs, path::Path, sync::Arc};

use anyhow::{bail, Context, Error, Result};
use quinn::{Connecting, Endpoint, ServerConfig};
use rustls_pemfile::Item::{ECKey, PKCS8Key, RSAKey};

#[tokio::main]
async fn main() {
    // Find paths if set
    let _qlogdir = var("QLOGDIR").ok().as_ref().map(|path| Path::new(path));
    let _logs = var("LOGS").ok().as_ref().map(|path| Path::new(path));
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

    // Handle new connections until the endpoint is closed
    while let Some(connection) = server.accept().await {
        let handle = handle_connection(www.clone(), connection);

        tokio::spawn(async move {
            if let Err(_why) = handle.await {
                todo!("log error")
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

    // Set key log file
    crypto_config.key_log = Arc::new(rustls::KeyLogFile::new());

    // Create server config
    let config = ServerConfig::with_crypto(Arc::new(crypto_config));

    Ok(config)
}

async fn handle_connection(_www: Arc<Path>, _connection: Connecting) -> Result<()> {
    Ok(())
}
