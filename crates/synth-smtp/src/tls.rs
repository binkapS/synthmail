//! Server TLS configuration for STARTTLS (RFC 3207).
//!
//! Certificates fail closed (like Postfix/Dovecot in this stack): if
//! `--tls-cert`/`--tls-key` are given but cannot be loaded, startup aborts
//! rather than serving a plaintext-only "submission" listener.

use std::sync::Arc;

use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio_rustls::TlsAcceptor;

/// Load a PEM certificate chain + private key and build the TLS acceptor.
pub fn load(cert_path: &str, key_path: &str) -> anyhow::Result<Arc<TlsAcceptor>> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_path)?
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("tls cert {cert_path}: {e}"))?;
    if certs.is_empty() {
        anyhow::bail!("tls cert {cert_path}: no certificates found");
    }
    let key = PrivateKeyDer::from_pem_file(key_path)
        .map_err(|e| anyhow::anyhow!("tls key {key_path}: {e}"))?;

    // Explicit ring provider: the workspace pins the rustls ring feature,
    // and naming it avoids "ambiguous default crypto provider" panics if
    // another provider ever enters the dependency tree.
    let config =
        ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
            .with_safe_default_protocol_versions()
            .map_err(|e| anyhow::anyhow!("tls: protocol versions: {e}"))?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| anyhow::anyhow!("tls: invalid cert/key pair: {e}"))?;

    Ok(Arc::new(TlsAcceptor::from(Arc::new(config))))
}
