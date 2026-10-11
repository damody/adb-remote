use anyhow::{Context, Result};
use std::{fs, path::Path};
use tonic::transport::{Certificate, ClientTlsConfig, Identity, ServerTlsConfig};

pub fn load_server_tls(cert_path: &Path, key_path: &Path) -> Result<ServerTlsConfig> {
    let cert = fs::read(cert_path).with_context(|| format!("read cert {}", cert_path.display()))?;
    let key = fs::read(key_path).with_context(|| format!("read key {}", key_path.display()))?;
    Ok(ServerTlsConfig::new().identity(Identity::from_pem(cert, key)))
}

pub fn load_client_tls(ca_cert_path: &Path, domain_name: &str) -> Result<ClientTlsConfig> {
    let ca = fs::read(ca_cert_path)
        .with_context(|| format!("read CA cert {}", ca_cert_path.display()))?;
    Ok(ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca))
        .domain_name(domain_name.to_string()))
}
