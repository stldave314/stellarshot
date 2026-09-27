// SPDX-License-Identifier: GPL-3.0-only

//! TLS for the web interface: a self-signed certificate generated once and
//! reused after, or a certificate and key the user supplies instead.
//!
//! The self-signed certificate is written to disk rather than regenerated on
//! every daemon start: regenerating it would invalidate any trust exception
//! a browser was given for the previous one on every single restart.

use std::io;
use std::path::{Path, PathBuf};

use axum_server::tls_rustls::RustlsConfig;

use crate::debug::WEB;
use crate::debug_log;

/// The certificate and key to serve TLS with: `custom` if both are given,
/// otherwise the self-signed pair, generating one first if this is the first
/// time.
pub async fn config(custom: Option<(&Path, &Path)>) -> io::Result<RustlsConfig> {
    let (cert, key) = match custom {
        Some((cert, key)) => (cert.to_path_buf(), key.to_path_buf()),
        None => {
            let dir =
                data_dir().ok_or_else(|| io::Error::other("no data directory for this user"))?;
            self_signed_paths(&dir)?
        }
    };
    RustlsConfig::from_pem_file(cert, key).await
}

/// Where the user's own data files live.
fn data_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|data| data.join("stellarshot/web"))
}

/// The self-signed certificate and key under `dir`, generating them first if
/// neither exists yet. A half-generated pair (one file present, one not — an
/// interrupted first run) is regenerated rather than served as-is.
///
/// `pub(crate)` so `super::web`'s own tests can build a real certificate for
/// a real end-to-end TLS handshake without going through `config`'s "custom"
/// path, which is meant for a certificate that already exists somewhere.
pub(crate) fn self_signed_paths(dir: &Path) -> io::Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    if !cert_path.exists() || !key_path.exists() {
        generate(&cert_path, &key_path)?;
    }
    Ok((cert_path, key_path))
}

/// Write a fresh self-signed certificate and key to `cert_path`/`key_path`,
/// valid for this machine's hostname, its mDNS name, and `localhost`.
fn generate(cert_path: &Path, key_path: &Path) -> io::Result<()> {
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let names = vec![
        hostname.clone(),
        format!("{hostname}.local"),
        "localhost".to_owned(),
    ];
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(names).map_err(io::Error::other)?;
    write_private(key_path, signing_key.serialize_pem().as_bytes())?;
    std::fs::write(cert_path, cert.pem())?;
    debug_log!(
        WEB,
        "generated a self-signed certificate at {}",
        cert_path.display()
    );
    Ok(())
}

/// Write `bytes` to `path`, readable only by this user: a private key must
/// never be group- or world-readable.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?
        .write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_pair_is_generated_and_both_files_are_written() {
        let dir = tempfile::TempDir::new().unwrap();
        let (cert, key) = self_signed_paths(dir.path()).unwrap();
        assert!(cert.exists());
        assert!(key.exists());
        assert!(
            std::fs::read_to_string(&cert)
                .unwrap()
                .contains("CERTIFICATE")
        );
        assert!(
            std::fs::read_to_string(&key)
                .unwrap()
                .contains("PRIVATE KEY")
        );
    }

    #[test]
    fn an_existing_pair_is_reused_rather_than_regenerated() {
        let dir = tempfile::TempDir::new().unwrap();
        let (cert, _key) = self_signed_paths(dir.path()).unwrap();
        let first_cert = std::fs::read_to_string(&cert).unwrap();

        let (cert_again, _) = self_signed_paths(dir.path()).unwrap();

        assert_eq!(cert, cert_again);
        assert_eq!(
            std::fs::read_to_string(&cert).unwrap(),
            first_cert,
            "a second call must not replace a certificate a browser may already trust"
        );
    }

    #[test]
    fn a_half_generated_pair_is_replaced_rather_than_served_broken() {
        let dir = tempfile::TempDir::new().unwrap();
        let (_cert, key) = self_signed_paths(dir.path()).unwrap();
        std::fs::remove_file(&key).unwrap();

        let (cert_again, key_again) = self_signed_paths(dir.path()).unwrap();

        assert!(cert_again.exists());
        assert!(key_again.exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_private_key_is_not_readable_by_anyone_else() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let (_, key) = self_signed_paths(dir.path()).unwrap();
        let mode = std::fs::metadata(&key).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }

    #[tokio::test]
    async fn a_custom_certificate_and_key_are_used_when_both_are_given() {
        // A self-signed pair stands in for "a certificate the user
        // supplied": `config`'s custom path only ever reads the files it is
        // given, it does not care how they were produced.
        let dir = tempfile::TempDir::new().unwrap();
        let (cert, key) = self_signed_paths(dir.path()).unwrap();
        assert!(config(Some((&cert, &key))).await.is_ok());
    }

    #[tokio::test]
    async fn a_missing_custom_certificate_fails_rather_than_falling_back() {
        // Falling back silently to the self-signed certificate when a
        // configured custom one cannot be read would hide a real
        // misconfiguration behind a certificate the operator did not choose.
        let result = config(Some((
            Path::new("/nonexistent/cert.pem"),
            Path::new("/nonexistent/key.pem"),
        )))
        .await;
        assert!(result.is_err());
    }
}
