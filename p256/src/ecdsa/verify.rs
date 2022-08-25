//! ECDSA verification support.

use super::{recoverable, Error, Signature};
use crate::{
    AffinePoint, CompressedPoint, EncodedPoint, NistP256, ProjectivePoint, PublicKey, Scalar,
};
use ecdsa_core::{hazmat::VerifyPrimitive, signature};
use elliptic_curve::{
    bigint::U256,
    consts::U32,
    ops::{Invert, LinearCombination, Reduce},
    sec1::ToEncodedPoint,
};
use signature::{digest::Digest, DigestVerifier};

#[cfg(feature = "sha256")]
use signature::PrehashSignature;

#[cfg(feature = "pkcs8")]
use crate::pkcs8::{self, DecodePublicKey};

#[cfg(feature = "pem")]
use core::str::FromStr;

#[cfg(all(feature = "pem", feature = "serde"))]
#[cfg_attr(docsrs, doc(cfg(all(feature = "pem", feature = "serde"))))]
use elliptic_curve::serde::{de, ser, Deserialize, Serialize};

/// ECDSA/P-256 verification key (i.e. public key)
///
/// # `serde` support
///
/// When the `serde` feature of this crate is enabled, the `Serialize` and
/// `Deserialize` traits are impl'd for this type.
///
/// The serialization is binary-oriented and supports ASN.1 DER-encoded
/// X.509 Subject Public Key Info (SPKI) as the encoding format.
///
/// For a more text-friendly encoding of public keys, use
/// [`elliptic_curve::JwkEcKey`] instead.
#[cfg_attr(docsrs, doc(cfg(feature = "ecdsa")))]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct VerifyingKey {
    /// Core ECDSA verify key
    pub(super) inner: ecdsa_core::VerifyingKey<NistP256>,
}

impl VerifyingKey {
    /// Initialize [`VerifyingKey`] from a SEC1-encoded public key.
    pub fn from_sec1_bytes(bytes: &[u8]) -> Result<Self, Error> {
        ecdsa_core::VerifyingKey::from_sec1_bytes(bytes).map(|key| VerifyingKey { inner: key })
    }

    /// Initialize [`VerifyingKey`] from a SEC1 [`EncodedPoint`].
    // TODO(tarcieri): switch to using `FromEncodedPoint` trait?
    pub fn from_encoded_point(public_key: &EncodedPoint) -> Result<Self, Error> {
        ecdsa_core::VerifyingKey::from_encoded_point(public_key)
            .map(|key| VerifyingKey { inner: key })
    }

    /// Serialize this [`VerifyingKey`] as a SEC1-encoded bytestring
    /// (with point compression applied)
    pub fn to_bytes(&self) -> CompressedPoint {
        CompressedPoint::clone_from_slice(EncodedPoint::from(self).as_bytes())
    }
}

#[cfg(feature = "sha256")]
impl<S> signature::Verifier<S> for VerifyingKey
where
    S: PrehashSignature,
    Self: DigestVerifier<S::Digest, S>,
{
    fn verify(&self, msg: &[u8], signature: &S) -> Result<(), Error> {
        self.verify_digest(S::Digest::new().chain(msg), signature)
    }
}

impl<D> DigestVerifier<D, Signature> for VerifyingKey
where
    D: Digest<OutputSize = U32>,
{
    fn verify_digest(&self, digest: D, signature: &Signature) -> Result<(), Error> {
        self.inner.verify_digest(digest, signature)
    }
}

impl<D> DigestVerifier<D, recoverable::Signature> for VerifyingKey
where
    D: Digest<OutputSize = U32>,
{
    fn verify_digest(&self, digest: D, signature: &recoverable::Signature) -> Result<(), Error> {
        self.inner
            .verify_digest(digest, &Signature::from(*signature))
    }
}

impl VerifyPrimitive<NistP256> for AffinePoint {
    fn verify_prehashed(&self, z: Scalar, signature: &Signature) -> Result<(), Error> {
        let (r, s) = signature.split_scalars();

        let s_inv = s.invert().unwrap();
        let u1 = z * s_inv;
        let u2 = *r * s_inv;

        let x = ProjectivePoint::lincomb(
            &ProjectivePoint::GENERATOR,
            &u1,
            &ProjectivePoint::from(self),
            &u2,
        )
        .to_affine()
        .x;

        if <Scalar as Reduce<U256>>::from_be_bytes_reduced(x.to_bytes()).eq(&r) {
            Ok(())
        } else {
            Err(Error::new())
        }
    }
}

impl From<PublicKey> for VerifyingKey {
    fn from(public_key: PublicKey) -> VerifyingKey {
        Self {
            inner: public_key.into(),
        }
    }
}

impl From<&PublicKey> for VerifyingKey {
    fn from(public_key: &PublicKey) -> VerifyingKey {
        VerifyingKey::from(*public_key)
    }
}

impl From<VerifyingKey> for PublicKey {
    fn from(verifying_key: VerifyingKey) -> PublicKey {
        verifying_key.inner.into()
    }
}

impl From<&VerifyingKey> for PublicKey {
    fn from(verifying_key: &VerifyingKey) -> PublicKey {
        verifying_key.inner.into()
    }
}

impl From<&AffinePoint> for VerifyingKey {
    fn from(affine_point: &AffinePoint) -> VerifyingKey {
        VerifyingKey::from_encoded_point(&affine_point.to_encoded_point(false)).unwrap()
    }
}

impl From<ecdsa_core::VerifyingKey<NistP256>> for VerifyingKey {
    fn from(verifying_key: ecdsa_core::VerifyingKey<NistP256>) -> VerifyingKey {
        VerifyingKey {
            inner: verifying_key,
        }
    }
}

impl From<&VerifyingKey> for EncodedPoint {
    fn from(verifying_key: &VerifyingKey) -> EncodedPoint {
        verifying_key.to_encoded_point(true)
    }
}

impl ToEncodedPoint<NistP256> for VerifyingKey {
    fn to_encoded_point(&self, compress: bool) -> EncodedPoint {
        self.inner.to_encoded_point(compress)
    }
}

impl TryFrom<&EncodedPoint> for VerifyingKey {
    type Error = Error;

    fn try_from(encoded_point: &EncodedPoint) -> Result<Self, Error> {
        Self::from_encoded_point(encoded_point)
    }
}

#[cfg(feature = "pkcs8")]
#[cfg_attr(docsrs, doc(cfg(feature = "pkcs8")))]
impl TryFrom<pkcs8::SubjectPublicKeyInfo<'_>> for VerifyingKey {
    type Error = pkcs8::spki::Error;

    fn try_from(spki: pkcs8::SubjectPublicKeyInfo<'_>) -> pkcs8::spki::Result<Self> {
        PublicKey::try_from(spki).map(|pk| Self { inner: pk.into() })
    }
}

#[cfg(feature = "pkcs8")]
#[cfg_attr(docsrs, doc(cfg(feature = "pkcs8")))]
impl DecodePublicKey for VerifyingKey {}

#[cfg(feature = "pem")]
#[cfg_attr(docsrs, doc(cfg(feature = "pem")))]
impl FromStr for VerifyingKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        Self::from_public_key_pem(s).map_err(|_| Error::new())
    }
}

#[cfg(all(feature = "pem", feature = "serde"))]
#[cfg_attr(docsrs, doc(cfg(all(feature = "pem", feature = "serde"))))]
impl Serialize for VerifyingKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

#[cfg(all(feature = "pem", feature = "serde"))]
#[cfg_attr(docsrs, doc(cfg(all(feature = "pem", feature = "serde"))))]
impl<'de> Deserialize<'de> for VerifyingKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        ecdsa_core::VerifyingKey::<NistP256>::deserialize(deserializer).map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::VerifyingKey;
    use crate::{test_vectors::ecdsa::ECDSA_TEST_VECTORS, NistP256};
    use ecdsa_core::signature::Verifier;
    use hex_literal::hex;

    ecdsa_core::new_verification_test!(NistP256, ECDSA_TEST_VECTORS);

    /// Wycheproof tcId: 304
    #[test]
    fn malleability_edge_case_valid() {
        let verifying_key_bytes =
            hex!("02c156afee1ce52ef83a0dd168c1144eb20008697e6664fa132ba23c128cce8055");
        let verifying_key = VerifyingKey::from_sec1_bytes(&verifying_key_bytes).unwrap();

        let msg = hex!("313233343030");
        let sig = Signature::from_der(&hex!("30440220784eea04d4a9e68260ba55b39277b2221db3793e47ec5c9301e43b45c7285792022016c4c411c20aa62c314ad383d00aa1e6c145641d7ce10e52075fb10e7d8bec2b")).unwrap();
        assert!(sig.normalize_s().is_none()); // Ensure signature is already normalized
        assert!(verifying_key.verify(&msg, &sig).is_ok());
    }
}
