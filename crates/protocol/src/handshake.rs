use rkyv::{Archive, Deserialize, Serialize};

/// `POST` with an empty body, answered with [`Hello`].
pub const HELLO: &str = "/rtc/hello";

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Hello {
    pub nonce: Vec<u8>,
    /// The STUN server tetherd uses, as `host:port`. The page uses it too,
    /// so each side can reach the other through a NAT.
    pub stun: Option<String>,
}

/// `POST` with [`Connect`], answered with [`Admitted`].
pub const CONNECT: &str = "/rtc/connect";

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub struct Connect {
    /// P-256, as SPKI.
    pub key: Vec<u8>,
    /// The output of [`signed`] over the other fields, signed with the
    /// private key (r and s, 32 bytes each). Signing the offer also signs the
    /// fingerprint of the DTLS certificate it carries, which ties the
    /// connection to this key.
    pub signature: Vec<u8>,
    pub name: String,
    pub kind: String,
    /// Each nonce [`Hello`] gives is valid for one attempt within a short
    /// time.
    pub nonce: Vec<u8>,
    pub offer: String,
    /// The [`code_proof`], for which a tetherd in setup mode trusts the
    /// page. The code itself is never sent, because this may go over plain
    /// HTTP.
    pub proof: Option<Vec<u8>>,
}

pub fn signed(nonce: &[u8], offer: &str, proof: Option<&[u8]>) -> Vec<u8> {
    [nonce, offer.as_bytes(), proof.unwrap_or_default()].concat()
}

/// The SHA-256 of the code with this nonce and offer, which cannot be
/// replayed for another offer or nonce.
pub fn code_proof(code: &str, nonce: &[u8], offer: &str) -> Vec<u8> {
    use sha2::Digest;

    sha2::Sha256::new()
        .chain_update(code.trim().to_uppercase().as_bytes())
        .chain_update(nonce)
        .chain_update(offer.as_bytes())
        .finalize()
        .to_vec()
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Admitted {
    /// `you` is the page's own fingerprint.
    Answer { answer: String, you: String },
    /// The page is not trusted yet and should ask again shortly.
    Gate(Gate),
}

#[derive(Clone, Debug, PartialEq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
pub enum Gate {
    /// Nothing is trusted yet, or tetherd was started with `--setup`. The
    /// setup code tetherd printed makes this page trusted. `primary` is
    /// tetherd's fingerprint, `you` the page's, and `missed` is true if the
    /// last code given was wrong.
    Setup {
        primary: String,
        you: String,
        missed: bool,
    },
    /// An already trusted page has to approve it.
    Waiting { you: String },
}
