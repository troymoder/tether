use std::path::Path;

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use sha2::{Digest, Sha256};

pub(crate) fn check(key: &[u8], nonce: &[u8], signature: &[u8]) -> Result<String> {
    let verifying = VerifyingKey::from_public_key_der(key).context("invalid key")?;
    let signature = Signature::from_slice(signature).context("invalid signature")?;

    verifying
        .verify(nonce, &signature)
        .context("invalid signature")?;

    Ok(fingerprint(key))
}

/// `SHA256:` and the base64 hash of the key as SPKI, as ssh writes it.
pub(super) fn fingerprint(spki: &[u8]) -> String {
    format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(spki)))
}

/// The file holds the private key's 32 bytes.
pub(super) fn own_key(file: &Path) -> Result<SigningKey> {
    if let Ok(kept) = std::fs::read(file) {
        return SigningKey::from_slice(&kept)
            .with_context(|| format!("invalid key: {}", file.display()));
    }

    let key = loop {
        if let Ok(key) = SigningKey::from_slice(&random::<32>()) {
            break key;
        }
    };

    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }

    write_private(file, &key.to_bytes())?;

    Ok(key)
}

fn write_private(file: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(file)
        .with_context(|| file.display().to_string())?
        .write_all(bytes)?;

    Ok(())
}

#[cfg(test)]
pub(super) fn sign(key: &SigningKey, nonce: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    use p256::ecdsa::signature::Signer;
    use p256::pkcs8::EncodePublicKey;

    let public = key.verifying_key().to_public_key_der().context("key")?;
    let signature: Signature = key.sign(nonce);

    Ok((public.as_bytes().to_vec(), signature.to_bytes().to_vec()))
}

pub(crate) fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];

    getrandom::fill(&mut bytes).expect("no randomness");

    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_a_signed_nonce() {
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        let (public, signature) = sign(&key, b"nonce").unwrap();

        assert_eq!(
            check(&public, b"nonce", &signature).unwrap(),
            fingerprint(&public)
        );
        assert!(check(&public, b"other", &signature).is_err());
    }
}
