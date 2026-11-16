//! The enrolment a VM's pairing needs, its token and its one-time secret,
//! kept in the account's Keychain under the VM's identifier until the
//! pairing completes, then deleted. Never asked for: the Keychain is used
//! with its prompts off, so a prompt is a failure.
use atlas_core::{
    desktop::{DesktopId, RecordKey},
    error::{Code, Error, Result},
    files::Token,
};
use atlas_native::keychain::{self, Item};
use atlas_session::keys::{KEY_LEN, PairingSecret};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;

/// The Keychain service enrolments are kept under, each as its VM's
/// identifier.
const SERVICE: &str = "com.insanearts.atlas.enrolment";

/// The most read of a secret given on standard input.
const GIVEN: u64 = 1024;

/// An enrolment under way: the token the guest's install was given, and
/// the secret its pairing proves.
#[derive(Debug)]
pub struct Enrolling {
    pub change: Token,
    pub secret: PairingSecret,
}

/// Where the station keeps a VM's enrolment until it is paired.
pub trait Enrolments: Send + Sync {
    /// Keep `enrolling` as `vm`'s, in place of any kept before.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be kept.
    fn keep(&self, vm: &DesktopId, enrolling: &Enrolling) -> Result<()>;

    /// `vm`'s enrolment, while it is kept.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be read, or what is kept is damaged.
    fn kept(&self, vm: &DesktopId) -> Result<Option<Enrolling>>;

    /// Delete `vm`'s enrolment: whether there was one.
    ///
    /// # Errors
    ///
    /// Fails when it cannot be deleted.
    fn forget(&self, vm: &DesktopId) -> Result<bool>;
}

/// The enrolments in the account's Keychain.
#[derive(Clone, Copy, Debug, Default)]
pub struct Keychain;

/// An enrolment as the Keychain item holds it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    enrolment: Token,
    secret: String,
}

impl Keychain {
    fn item(vm: &DesktopId) -> Item<'_> {
        Item {
            service: SERVICE,
            account: vm.as_str(),
        }
    }
}

impl Enrolments for Keychain {
    fn keep(&self, vm: &DesktopId, enrolling: &Enrolling) -> Result<()> {
        let stored = Stored {
            enrolment: enrolling.change.clone(),
            secret: atlas_core::name::hex(enrolling.secret.as_bytes()),
        };
        keychain::write(Self::item(vm), &serde_json::to_vec(&stored)?)
    }

    fn kept(&self, vm: &DesktopId) -> Result<Option<Enrolling>> {
        let Some(bytes) = keychain::read(Self::item(vm))? else {
            return Ok(None);
        };
        let damaged = || {
            Error::new(
                Code::Identity,
                "The VM's enrolment in the Keychain is damaged; enrol it again.",
            )
        };
        let stored: Stored = serde_json::from_slice(&bytes).map_err(|_| damaged())?;
        let secret = decoded(&stored.secret).ok_or_else(damaged)?;
        Ok(Some(Enrolling {
            change: stored.enrolment,
            secret,
        }))
    }

    fn forget(&self, vm: &DesktopId) -> Result<bool> {
        keychain::delete(Self::item(vm))
    }
}

/// The secret `text` writes as hexadecimal digits, two per byte.
fn decoded(text: &str) -> Option<PairingSecret> {
    let digits = text.as_bytes();
    if digits.len() != KEY_LEN * 2 || !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let mut bytes = [0; KEY_LEN];
    for (byte, pair) in bytes.iter_mut().zip(digits.chunks_exact(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(PairingSecret::from_bytes(bytes))
}

/// The VM a station record key `vm-<id>` names.
///
/// # Errors
///
/// Refuses any other key.
pub fn vm_of(key: &str) -> Result<DesktopId> {
    RecordKey::parse(key)
        .and_then(|key| key.vm_id())
        .ok_or_else(|| {
            Error::new(
                Code::BadRequest,
                "That names no VM desktop: its key is vm- and its identifier.",
            )
        })
}

/// Keep the enrolment `enrolment` of the VM keyed `key`, its secret read
/// from `input`, never from the command line, in `store`.
///
/// # Errors
///
/// Refuses a key that names no VM, a token that is not one, or a secret
/// that is not 64 hexadecimal digits; fails when it cannot be kept.
pub fn enrol(
    store: &impl Enrolments,
    key: &str,
    enrolment: &str,
    input: &mut impl Read,
) -> Result<Value> {
    let vm = vm_of(key)?;
    let change = Token::parse(enrolment)
        .ok_or_else(|| Error::new(Code::BadRequest, "That names no valid enrolment."))?;
    let mut given = String::new();
    input
        .take(GIVEN)
        .read_to_string(&mut given)
        .map_err(|error| {
            Error::new(
                Code::BadRequest,
                "The enrolment's secret could not be read.",
            )
            .with_source(error)
        })?;
    let secret = decoded(given.trim()).ok_or_else(|| {
        Error::new(
            Code::BadRequest,
            "The enrolment's secret on standard input is not 64 hexadecimal digits.",
        )
    })?;
    store.keep(&vm, &Enrolling { change, secret })?;
    Ok(json!({"ok": true, "key": key, "enrolment": enrolment, "kept": true}))
}

#[cfg(test)]
#[path = "enrolment_tests.rs"]
mod tests;
