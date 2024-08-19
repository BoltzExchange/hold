use anyhow::Result;
use log::{debug, trace};
use rcgen::{CertificateParams, KeyPair};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tonic::transport::{Certificate, Identity};

pub fn load_certificates(base_path: PathBuf) -> Result<(Identity, Certificate)> {
    debug!("Loading gRPC certificates from: {:?}", base_path);
    let base = Path::new(base_path.as_path());

    if !base.exists() {
        fs::create_dir_all(base)?;
    }

    let (ca_key, ca_cert) = generate_or_load_certificate("Hold Root CA", base, "ca", None)?;
    let ca_keypair = KeyPair::from_pem(&String::from_utf8_lossy(&ca_key))?;
    let ca = (
        &ca_keypair,
        &CertificateParams::from_ca_cert_pem(&String::from_utf8_lossy(&ca_cert.clone()))?
            .self_signed(&ca_keypair)?,
    );

    let (server_key, server_cert) =
        generate_or_load_certificate("Hold gRPC server", base, "server", Some(ca))?;
    generate_or_load_certificate("Hold gRPC client", base, "client", Some(ca))?;

    trace!("Loaded certificates");
    Ok((
        Identity::from_pem(server_cert, server_key),
        Certificate::from_pem(ca_cert),
    ))
}

fn generate_or_load_certificate(
    name: &str,
    directory: &Path,
    file_name: &str,
    parent: Option<(&KeyPair, &rcgen::Certificate)>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let key_path = directory.join(format!("{}-key.pem", file_name));
    let cert_path = directory.join(format!("{}.pem", file_name));

    if !key_path.exists() || !cert_path.exists() {
        debug!("Creating new certificates for: {}", name);
        return generate_certificate(name, key_path, cert_path, parent);
    }

    trace!("Found existing certificates for: {}", name);
    Ok((fs::read(key_path)?, fs::read(cert_path)?))
}

fn generate_certificate(
    name: &str,
    key_path: PathBuf,
    cert_path: PathBuf,
    parent: Option<(&KeyPair, &rcgen::Certificate)>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let key_pair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;

    let mut key_file = File::create(key_path.clone())?;
    let mut perms = fs::metadata(key_path.clone()).unwrap().permissions();
    perms.set_mode(0o600);
    fs::set_permissions(&key_path, perms)?;

    key_file.write_all(key_pair.serialize_pem().as_bytes())?;

    let mut cert_params = CertificateParams::new(vec![
        "hold".to_string(),
        crate::utils::built_info::PKG_NAME.to_string(),
        "localhost".to_string(),
        "127.0.0.1".to_string(),
    ])?;
    cert_params.is_ca = if parent.is_none() {
        rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained)
    } else {
        rcgen::IsCa::NoCa
    };
    cert_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, name);

    let cert = match parent {
        None => cert_params.self_signed(&key_pair),
        Some((ca_key, ca_cert)) => cert_params.signed_by(&key_pair, ca_cert, ca_key),
    }?;

    File::create(cert_path)?.write_all(cert.pem().as_bytes())?;

    Ok((
        Vec::from(key_pair.serialize_pem().as_bytes()),
        Vec::from(cert.pem().as_bytes()),
    ))
}
