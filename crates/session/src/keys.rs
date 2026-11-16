//! Each end's long-lived key, where it is kept, and the one-time secret a
//! pairing proves.
use atlas_core::{
    error::{Code, Error, Result},
    session::key::PeerKey,
};
use snow::{
    params::DHChoice,
    resolvers::{CryptoResolver, DefaultResolver},
};
use std::fmt;

/// The length of a key, either half, and of a pairing secret.
pub const KEY_LEN: usize = 32;

/// One end's secret key. It is never printed.
pub struct SecretKey([u8; KEY_LEN]);

impl SecretKey {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// The key's bytes, for a store to keep.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretKey(..)")
    }
}

/// An end's key pair.
#[derive(Debug)]
pub struct Keypair {
    secret: SecretKey,
    public: PeerKey,
}

impl Keypair {
    /// A new key pair from the system's random source.
    ///
    /// # Errors
    ///
    /// Fails when the random source does.
    pub fn generate() -> Result<Self> {
        let mut bytes = [0; KEY_LEN];
        fill_random(&mut bytes)?;
        Self::from_secret(SecretKey(bytes))
    }

    /// The pair whose secret half is `secret`.
    ///
    /// # Errors
    ///
    /// Fails only when this build has no Curve25519, which it always has.
    pub fn from_secret(secret: SecretKey) -> Result<Self> {
        let mut dh = DefaultResolver
            .resolve_dh(&DHChoice::Curve25519)
            .ok_or_else(|| Error::new(Code::Io, "This build cannot make session keys."))?;
        dh.set(&secret.0);
        let public = PeerKey::from_slice(dh.pubkey()).ok_or_else(|| {
            Error::new(
                Code::Io,
                "This build made a session key of the wrong length.",
            )
        })?;
        Ok(Self { secret, public })
    }

    #[must_use]
    pub const fn public(&self) -> PeerKey {
        self.public
    }

    pub(crate) const fn secret(&self) -> &[u8; KEY_LEN] {
        &self.secret.0
    }
}

/// Where an end keeps its secret key.
pub trait KeyStore {
    /// The key kept here, if there is one.
    ///
    /// # Errors
    ///
    /// Fails when the store cannot be read; a store with no key is `None`.
    fn load(&self) -> Result<Option<SecretKey>>;

    /// Keep `secret` here from now on.
    ///
    /// # Errors
    ///
    /// Fails when the store cannot be written.
    fn save(&self, secret: &SecretKey) -> Result<()>;
}

/// This end's key: the one kept in `store`, or a new one kept there from
/// now on.
///
/// # Errors
///
/// Fails when the store cannot be read or written, or no key can be made.
pub fn own_key(store: &impl KeyStore) -> Result<Keypair> {
    if let Some(secret) = store.load()? {
        return Keypair::from_secret(secret);
    }
    let made = Keypair::generate()?;
    store.save(&made.secret)?;
    Ok(made)
}

/// Where an end keeps a key that, once made, is never replaced.
pub trait KeptOnce {
    /// The key kept here, if there is one.
    ///
    /// # Errors
    ///
    /// Fails when the store cannot be read; a store with no key is `None`.
    fn load(&self) -> Result<Option<SecretKey>>;

    /// Keep `secret` here only when no key is kept yet: whether it was.
    ///
    /// # Errors
    ///
    /// Fails when the store cannot be written.
    fn add(&self, secret: &SecretKey) -> Result<bool>;
}

/// This end's key: the one kept in `store`, or a new one kept there only
/// where none is. One kept there meanwhile is the key, never replaced.
///
/// # Errors
///
/// Fails when the store cannot be read or written, or no key can be made.
pub fn kept_once(store: &impl KeptOnce) -> Result<Keypair> {
    if let Some(secret) = store.load()? {
        return Keypair::from_secret(secret);
    }
    let made = Keypair::generate()?;
    if store.add(&made.secret)? {
        return Ok(made);
    }
    let kept = store.load()?.ok_or_else(|| {
        Error::new(
            Code::Identity,
            "A session key was kept and then went; it is not made again.",
        )
    })?;
    Keypair::from_secret(kept)
}

/// The one-time secret a pairing proves: the guest shows it holds the one
/// the station made.
pub struct PairingSecret([u8; KEY_LEN]);

impl PairingSecret {
    /// A new secret from the system's random source.
    ///
    /// # Errors
    ///
    /// Fails when the random source does.
    pub fn generate() -> Result<Self> {
        let mut bytes = [0; KEY_LEN];
        fill_random(&mut bytes)?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// The secret's bytes, to hand to the guest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for PairingSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PairingSecret(..)")
    }
}

fn fill_random(bytes: &mut [u8]) -> Result<()> {
    DefaultResolver
        .resolve_rng()
        .ok_or_else(|| {
            Error::new(
                Code::Io,
                "This build has no random source for session keys.",
            )
        })?
        .try_fill_bytes(bytes)
        .map_err(|error| Error::new(Code::Io, "The random source failed.").with_source(error))
}

/// A store kept in memory, for tests.
#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct MemoryStore(std::sync::Mutex<Option<[u8; KEY_LEN]>>);

#[cfg(any(test, feature = "test-support"))]
impl KeyStore for MemoryStore {
    fn load(&self) -> Result<Option<SecretKey>> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .map(SecretKey))
    }

    fn save(&self, secret: &SecretKey) -> Result<()> {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(secret.0);
        Ok(())
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
