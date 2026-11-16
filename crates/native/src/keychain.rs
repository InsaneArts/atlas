//! Secrets kept in the account's Keychain as generic passwords, each named
//! by a service and an account. An item this program makes trusts only
//! Atlas's signature: another program, Apple's `security` tool among them,
//! needs the Keychain's password to read it. Nothing here ever asks the
//! person: an item that would need them, a locked Keychain or an item
//! another program made, fails instead, so a key another program planted
//! is never taken for Atlas's own.
#![allow(
    unsafe_code,
    reason = "FFI: Security.framework's SecItem calls have no safe binding"
)]

use atlas_core::error::{Code, Error, Result};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFData, CFDictionary, CFRetained, CFString, CFType,
};
use objc2_security::{
    SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate, SecKeychainItem,
    errSecDuplicateItem, errSecItemNotFound, kSecAttrAccount, kSecAttrService, kSecClass,
    kSecClassGenericPassword, kSecMatchItemList, kSecMatchLimit, kSecMatchLimitOne, kSecReturnData,
    kSecReturnRef, kSecUseDataProtectionKeychain, kSecValueData,
};
use std::{
    ptr::null,
    sync::{Mutex, PoisonError},
};

mod made;

/// Held while the Keychain is used with the person's prompts turned off:
/// the setting is the whole process's.
static QUIET: Mutex<()> = Mutex::new(());

/// Where a secret is kept: a service and an account within it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item<'a> {
    pub service: &'a str,
    pub account: &'a str,
}

/// The secret kept as `item`; none when there is no such item.
///
/// # Errors
///
/// Fails when the Keychain cannot be read or refuses this program.
pub fn read(item: Item<'_>) -> Result<Option<Vec<u8>>> {
    let Some(found) = found(item)? else {
        return Ok(None);
    };
    if !made::here(&found)? {
        return Err(foreign());
    }
    // SAFETY: the keys and values are the framework's own constants.
    let (limit, one, data, list) = unsafe {
        (
            kSecMatchLimit,
            kSecMatchLimitOne,
            kSecReturnData,
            kSecMatchItemList,
        )
    };
    let yes = CFBoolean::new(true);
    // The very item checked, never another one found by name since.
    let checked_one = CFArray::<SecKeychainItem>::from_objects(&[&*found]);
    let query = query(
        item,
        &[
            (limit, one.as_ref()),
            (data, yes.as_ref()),
            (list, checked_one.as_ref()),
        ],
    );
    let mut found: *const CFType = null();
    // SAFETY: the query maps the framework's keys to values of the types
    // they take; `found` is a valid place for the call's result.
    let status = quietly(|| unsafe { SecItemCopyMatching(query.as_opaque(), &mut found) })?;
    if status == errSecItemNotFound {
        return Ok(None);
    }
    checked(status, "read")?;
    // SAFETY: on success the call returned a retained object.
    let found = std::ptr::NonNull::new(found.cast_mut())
        .map(|found| unsafe { CFRetained::from_raw(found) })
        .ok_or_else(|| refused("read", status))?;
    let bytes = found
        .downcast::<CFData>()
        .map_err(|_| refused("read", status))?;
    Ok(Some(bytes.to_vec()))
}

/// Keep `secret` as `item` from now on, in place of anything Atlas kept
/// there.
///
/// # Errors
///
/// Fails when the Keychain cannot be written or refuses this program, as
/// it does an item another program made.
pub fn write(item: Item<'_>, secret: &[u8]) -> Result<()> {
    // SAFETY: the key is the framework's own constant.
    let value = unsafe { kSecValueData };
    let data = CFData::from_bytes(secret);
    let added = query(item, &[(value, data.as_ref())]);
    // SAFETY: the attributes map the framework's keys to values of the types
    // they take; no result is asked for.
    let status = quietly(|| unsafe { SecItemAdd(added.as_opaque(), std::ptr::null_mut()) })?;
    if status != errSecDuplicateItem {
        return checked(status, "write");
    }
    let Some(existing) = found(item)? else {
        return Err(refused("write", status));
    };
    if !made::here(&existing)? {
        return Err(foreign());
    }
    // SAFETY: the key is the framework's own constant.
    let list = unsafe { kSecMatchItemList };
    // The very item checked, never another one found by name since.
    let checked_one = CFArray::<SecKeychainItem>::from_objects(&[&*existing]);
    let found = query(item, &[(list, checked_one.as_ref())]);
    let changed = CFDictionary::<CFString, CFType>::from_slices(&[value], &[data.as_ref()]);
    // SAFETY: both dictionaries map the framework's keys to values of the
    // types they take.
    let status = quietly(|| unsafe { SecItemUpdate(found.as_opaque(), changed.as_opaque()) })?;
    checked(status, "write")
}

/// Keep `secret` as `item` only when there is no such item yet: whether it
/// was kept. One there already, whoever made it, is left as it is.
///
/// # Errors
///
/// Fails when the Keychain cannot be written or refuses this program.
pub fn add(item: Item<'_>, secret: &[u8]) -> Result<bool> {
    // SAFETY: the key is the framework's own constant.
    let value = unsafe { kSecValueData };
    let data = CFData::from_bytes(secret);
    let added = query(item, &[(value, data.as_ref())]);
    // SAFETY: the attributes map the framework's keys to values of the types
    // they take; no result is asked for.
    let status = quietly(|| unsafe { SecItemAdd(added.as_opaque(), std::ptr::null_mut()) })?;
    if status == errSecDuplicateItem {
        return Ok(false);
    }
    checked(status, "write").map(|()| true)
}

/// Remove `item`; whether there was one.
///
/// # Errors
///
/// Fails when the Keychain cannot be written or refuses this program.
pub fn delete(item: Item<'_>) -> Result<bool> {
    let found = query(item, &[]);
    // SAFETY: the query maps the framework's keys to values of the types
    // they take.
    let status = quietly(|| unsafe { SecItemDelete(found.as_opaque()) })?;
    if status == errSecItemNotFound {
        return Ok(false);
    }
    checked(status, "delete").map(|()| true)
}

/// The item `item` names, if there is one.
fn found(item: Item<'_>) -> Result<Option<CFRetained<SecKeychainItem>>> {
    // SAFETY: the keys and values are the framework's own constants.
    let (limit, one, reference) = unsafe { (kSecMatchLimit, kSecMatchLimitOne, kSecReturnRef) };
    let yes = CFBoolean::new(true);
    let query = query(item, &[(limit, one.as_ref()), (reference, yes.as_ref())]);
    let mut found: *const CFType = null();
    // SAFETY: the query maps the framework's keys to values of the types
    // they take; `found` is a valid place for the call's result.
    let status = quietly(|| unsafe { SecItemCopyMatching(query.as_opaque(), &mut found) })?;
    if status == errSecItemNotFound {
        return Ok(None);
    }
    checked(status, "read")?;
    // SAFETY: on success the call returned a retained object.
    let found = std::ptr::NonNull::new(found.cast_mut())
        .map(|found| unsafe { CFRetained::from_raw(found) })
        .ok_or_else(|| refused("read", status))?;
    found
        .downcast::<SecKeychainItem>()
        .map(Some)
        .map_err(|_| refused("read", status))
}

/// An item of that name that another program made.
fn foreign() -> Error {
    Error::new(
        Code::Identity,
        "A Keychain item Atlas keeps was made by another program; Atlas does not use it.",
    )
}

/// Run `call` with the Keychain's prompts to the person turned off, then
/// put the setting back as it was. The file Keychain honours only this
/// process-wide setting, not a query's own.
#[expect(
    deprecated,
    reason = "the file Keychain has no other way to refuse rather than prompt"
)]
fn quietly<T>(call: impl FnOnce() -> T) -> Result<T> {
    use objc2_security::SecKeychain;
    let _quiet = QUIET.lock().unwrap_or_else(PoisonError::into_inner);
    let mut before: u8 = 0;
    // SAFETY: `before` is a valid place for the setting.
    let status =
        unsafe { SecKeychain::user_interaction_allowed(std::ptr::NonNull::from(&mut before)) };
    checked(status, "use")?;
    // SAFETY: the call takes a plain flag.
    let status = unsafe { SecKeychain::set_user_interaction_allowed(false) };
    checked(status, "use")?;
    let _restore = Restore(before != 0);
    Ok(call())
}

/// Puts the Keychain's prompts back as they were, however the call ended.
struct Restore(bool);

impl Drop for Restore {
    #[expect(
        deprecated,
        reason = "the file Keychain has no other way to refuse rather than prompt"
    )]
    fn drop(&mut self) {
        // SAFETY: the call takes a plain flag.
        unsafe { objc2_security::SecKeychain::set_user_interaction_allowed(self.0) };
    }
}

/// The generic password `item` names in the account's own Keychain, with
/// `more` added.
fn query(
    item: Item<'_>,
    more: &[(&'static CFString, &CFType)],
) -> CFRetained<CFDictionary<CFString, CFType>> {
    // SAFETY: the keys and values are the framework's own constants.
    let (class, generic, service, account, protected) = unsafe {
        (
            kSecClass,
            kSecClassGenericPassword,
            kSecAttrService,
            kSecAttrAccount,
            kSecUseDataProtectionKeychain,
        )
    };
    let named = (
        CFString::from_str(item.service),
        CFString::from_str(item.account),
    );
    // The account's file Keychain, whose items trust the program that made
    // them; the data protection one needs a provisioning profile.
    let no = CFBoolean::new(false);
    let mut keys: Vec<&CFString> = vec![class, service, account, protected];
    let mut values: Vec<&CFType> = vec![
        generic.as_ref(),
        named.0.as_ref(),
        named.1.as_ref(),
        no.as_ref(),
    ];
    for (key, value) in more {
        keys.push(key);
        values.push(value);
    }
    CFDictionary::from_slices(&keys, &values)
}

/// `status` as a result of the Keychain's `doing`.
fn checked(status: i32, doing: &str) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(refused(doing, status))
    }
}

fn refused(doing: &str, status: i32) -> Error {
    Error::new(
        Code::Identity,
        format!("The Keychain refused to {doing} Atlas's item (status {status})."),
    )
}

#[cfg(test)]
#[path = "keychain_tests.rs"]
mod tests;
