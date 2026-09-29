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
use sha2::Digest;

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
    // `mode(0o700)` only applies to a directory this actually creates, the
    // same way `write_private`'s own mode only applied to a file it
    // created before that was fixed below — an already-existing `dir` (the
    // common case after the first run) keeps whatever mode it already has,
    // never loosened or tightened by this call.
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    if !cert_path.exists() || !key_path.exists() {
        generate(&cert_path, &key_path)?;
    }
    Ok((cert_path, key_path))
}

/// Write a fresh self-signed certificate and key to `cert_path`/`key_path`,
/// valid for this machine's hostname, its mDNS name, `localhost`, and —
/// unlike a plain `rcgen::generate_simple_self_signed`, which only ever
/// names hosts — the loopback addresses Settings actually shows in the
/// address it tells you to connect to (`https://127.0.0.1:port`): a
/// certificate with no IP SAN at all for the address you are told to visit
/// trains you to click through a browser's warning instead of noticing a
/// real one. Valid for `WEB_CERT_VALIDITY`, not `rcgen`'s own default
/// (1975 to 4096), which never actually expires and so never says anything.
/// `rcgen::CertificateParams::new` classifies each of these as either a DNS
/// name or an IP SAN by whether it parses as an [`std::net::IpAddr`] —
/// `127.0.0.1` and `::1` end up as IP SANs this way, without constructing
/// `rcgen::SanType` by hand.
fn cert_names(hostname: &str) -> Vec<String> {
    vec![
        hostname.to_owned(),
        format!("{hostname}.local"),
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
    ]
}

fn generate(cert_path: &Path, key_path: &Path) -> io::Result<()> {
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let names = cert_names(&hostname);
    let signing_key = rcgen::KeyPair::generate().map_err(io::Error::other)?;
    let mut params = rcgen::CertificateParams::new(names).map_err(io::Error::other)?;
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now
        + time::Duration::try_from(crate::constants::WEB_CERT_VALIDITY)
            .map_err(io::Error::other)?;
    let cert = params.self_signed(&signing_key).map_err(io::Error::other)?;
    write_private(key_path, signing_key.serialize_pem().as_bytes())?;
    std::fs::write(cert_path, cert.pem())?;
    debug_log!(
        WEB,
        "generated a self-signed certificate at {}, valid until {}",
        cert_path.display(),
        params.not_after
    );
    Ok(())
}

/// The certificate's SHA-256 fingerprint, hex-encoded — what a
/// trust-on-first-use check (`curl --pinnedpubkey`, a browser's own
/// certificate viewer) actually compares against, so Settings can show it
/// rather than leaving no way to verify the certificate at all beyond
/// trusting whatever is presented.
pub fn fingerprint(cert_path: &Path) -> io::Result<String> {
    use rustls_pki_types::CertificateDer;
    use rustls_pki_types::pem::PemObject;
    let der = CertificateDer::from_pem_file(cert_path).map_err(io::Error::other)?;
    Ok(sha2::Sha256::digest(&der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":"))
}

/// Write `bytes` to `path`, readable only by this user: a private key must
/// never be group- or world-readable.
///
/// `OpenOptions::mode` only takes effect when the open call actually
/// *creates* the file — reusing an existing `path` (regenerating `key.pem`
/// after only `cert.pem` went missing, say) would silently keep whatever
/// mode was already there instead. Written to a fresh temporary file
/// instead, which `mode(0o600)` always applies to since it is always newly
/// created, then moved into place: the rename replaces `path`'s directory
/// entry outright, so the file that ends up there is the new one, mode and
/// all, never a reused inode with a stale mode.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true).mode(0o600);
    atomicwrites::AtomicFile::new(path, atomicwrites::AllowOverwrite)
        .write_with_options(|file| file.write_all(bytes), options)
        .map_err(|err: atomicwrites::Error<io::Error>| match err {
            atomicwrites::Error::Internal(err) | atomicwrites::Error::User(err) => err,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loopback_addresses_are_named_alongside_the_host() {
        let names = cert_names("desktop");
        assert!(names.contains(&"127.0.0.1".to_owned()));
        assert!(names.contains(&"::1".to_owned()));
        assert!(names.contains(&"desktop".to_owned()));
        assert!(names.contains(&"desktop.local".to_owned()));
        assert!(names.contains(&"localhost".to_owned()));
    }

    #[test]
    fn a_generated_certificates_fingerprint_is_stable_and_well_formed() {
        let dir = tempfile::TempDir::new().unwrap();
        let (cert, _key) = self_signed_paths(dir.path()).unwrap();

        let first = fingerprint(&cert).unwrap();
        let second = fingerprint(&cert).unwrap();

        assert_eq!(first, second, "the same file must fingerprint the same");
        // 32 bytes, hex-encoded two characters each, joined by a colon
        // between every pair: 32 * 2 + 31 characters.
        assert_eq!(first.len(), 95, "{first}");
        assert!(
            first.chars().all(|c| c.is_ascii_hexdigit() || c == ':'),
            "{first}"
        );
    }

    #[test]
    fn two_generated_certificates_have_different_fingerprints() {
        let dir_a = tempfile::TempDir::new().unwrap();
        let dir_b = tempfile::TempDir::new().unwrap();
        let (cert_a, _) = self_signed_paths(dir_a.path()).unwrap();
        let (cert_b, _) = self_signed_paths(dir_b.path()).unwrap();

        assert_ne!(fingerprint(&cert_a).unwrap(), fingerprint(&cert_b).unwrap());
    }

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

    #[cfg(unix)]
    #[test]
    fn a_regenerated_key_is_not_readable_by_anyone_else_even_if_the_old_one_was() {
        // SEC-7: `OpenOptions::mode` only takes effect on the create, not on
        // reusing an existing path — a stale, loosely-permissioned key.pem
        // left over from before this fix (or, in principle, tampered with)
        // must not survive being "regenerated" over.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let (cert, key) = self_signed_paths(dir.path()).unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::remove_file(&cert).unwrap();

        let (_, key_again) = self_signed_paths(dir.path()).unwrap();

        assert_eq!(key, key_again);
        let mode = std::fs::metadata(&key_again).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }

    #[cfg(unix)]
    #[test]
    fn the_tls_directory_is_not_traversable_by_anyone_else() {
        use std::os::unix::fs::PermissionsExt;
        let parent = tempfile::TempDir::new().unwrap();
        let dir = parent.path().join("web");
        self_signed_paths(&dir).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "{mode:o}");
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
