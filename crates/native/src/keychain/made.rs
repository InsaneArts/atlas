//! Who made a Keychain item, and who may read it. Its partitions say who
//! made it: the Keychain gives an item its maker's partition (a signed
//! program's team, an unsigned one's code hash), and changing them takes
//! the Keychain's password. A team is shared by every program it signs, so
//! the entries that let a program read the item must name this program
//! alone too.
#![expect(
    deprecated,
    reason = "item access and partitions exist only in the deprecated file Keychain API"
)]

use super::{checked, refused};
use atlas_core::error::Result;
use objc2_core_foundation::{CFArray, CFData, CFDictionary, CFRetained, CFString, CFType};
use objc2_security::{
    SecACL, SecAccess, SecCSFlags, SecCode, SecKeychainItem, SecKeychainPromptSelector,
    SecRequirement, SecStaticCode, SecTrustedApplication, kSecACLAuthorizationAny,
    kSecACLAuthorizationDecrypt, kSecACLAuthorizationPartitionID, kSecCSSigningInformation,
    kSecCodeInfoTeamIdentifier, kSecCodeInfoUnique,
};
use std::ptr::{NonNull, null, null_mut};

/// Whether this program alone made `item` and may read it: its partitions
/// are this program's own and no other, and every entry that lets a
/// program read it names this program and no other.
pub(super) fn here(item: &SecKeychainItem) -> Result<bool> {
    let own_partition = own_partition().ok_or_else(|| refused("check", 0))?;
    let own_application = (own_application()?, own_requirement()?);
    // SAFETY: the keys are the framework's own constants.
    let (partition, decrypt, any) = unsafe {
        (
            kSecACLAuthorizationPartitionID,
            kSecACLAuthorizationDecrypt,
            kSecACLAuthorizationAny,
        )
    };
    let (mut partitioned, mut readers) = (false, 0);
    for entry in entries(item)?.iter() {
        // SAFETY: the entry is a valid access control entry.
        let authorizations: CFRetained<CFArray<CFString>> =
            unsafe { CFRetained::cast_unchecked(entry.authorizations()) };
        let grants = |wanted: &CFString| authorizations.iter().any(|kind| *kind == *wanted);
        let (applications, description) = contents(&entry)?;
        if grants(partition) {
            if listed(&description) != [own_partition.as_str()] {
                return Ok(false);
            }
            partitioned = true;
        }
        if grants(decrypt) || grants(any) {
            // No list at all lets every program read it.
            let Some(applications) = applications else {
                return Ok(false);
            };
            let only_this = !applications.is_empty()
                && applications
                    .iter()
                    .all(|application| is_this(&application, &own_application));
            if !only_this {
                return Ok(false);
            }
            readers += 1;
        }
    }
    Ok(partitioned && readers > 0)
}

/// The access control entries `item` keeps.
fn entries(item: &SecKeychainItem) -> Result<CFRetained<CFArray<SecACL>>> {
    let mut access: *mut SecAccess = null_mut();
    // SAFETY: `access` is a valid place for the call's result.
    let status = unsafe { item.copy_access(NonNull::from(&mut access)) };
    checked(status, "check")?;
    let access = NonNull::new(access).ok_or_else(|| refused("check", 0))?;
    // SAFETY: on success the call returned a retained access.
    let access = unsafe { CFRetained::from_raw(access) };
    let mut list: *const CFArray = null();
    // SAFETY: `list` is a valid place for the call's result.
    let status = unsafe { access.copy_acl_list(NonNull::from(&mut list)) };
    checked(status, "check")?;
    let list = NonNull::new(list.cast_mut()).ok_or_else(|| refused("check", 0))?;
    // SAFETY: on success the call returned a retained array of access
    // control entries.
    Ok(unsafe { CFRetained::cast_unchecked(CFRetained::from_raw(list)) })
}

/// The applications an entry trusts (none: any application) and its
/// description, which for partitions is their property list in
/// hexadecimal.
type Contents = (Option<CFRetained<CFArray<SecTrustedApplication>>>, String);

fn contents(entry: &SecACL) -> Result<Contents> {
    let (mut applications, mut description) = (null::<CFArray>(), null::<CFString>());
    let mut prompt = SecKeychainPromptSelector::empty();
    // SAFETY: each pointer is a valid place for the call's results.
    let status = unsafe {
        entry.copy_contents(
            NonNull::from(&mut applications),
            NonNull::from(&mut description),
            NonNull::from(&mut prompt),
        )
    };
    // SAFETY: the call returned a retained array of trusted applications,
    // or none.
    let applications = NonNull::new(applications.cast_mut()).map(|applications| unsafe {
        CFRetained::cast_unchecked(CFRetained::from_raw(applications))
    });
    checked(status, "check")?;
    // SAFETY: the call returned a retained string, or none.
    let description = NonNull::new(description.cast_mut())
        .map(|description| unsafe { CFRetained::from_raw(description) }.to_string())
        .unwrap_or_default();
    Ok((applications, description))
}

unsafe extern "C-unwind" {
    /// The code requirement a trusted application entry holds. Exported by
    /// Security, declared only in its private headers; an entry's path
    /// alone says nothing of which code it lets in.
    fn SecTrustedApplicationCopyRequirement(
        application: &SecTrustedApplication,
        requirement: *mut *mut SecRequirement,
    ) -> i32;
}

/// Whether `application` is this program: its path, and a requirement
/// exactly this program's designated one, never a broader one.
fn is_this(application: &SecTrustedApplication, own: &(Vec<u8>, String)) -> bool {
    let mut requirement: *mut SecRequirement = null_mut();
    // SAFETY: the entry is a valid trusted application; `requirement` is a
    // valid place for the call's result.
    let status = unsafe { SecTrustedApplicationCopyRequirement(application, &mut requirement) };
    let Some(requirement) = (status == 0).then_some(NonNull::new(requirement)).flatten() else {
        return false;
    };
    // SAFETY: on success the call returned a retained requirement.
    let requirement = unsafe { CFRetained::from_raw(requirement) };
    data(application).is_ok_and(|path| path == own.0)
        && text(&requirement).is_ok_and(|requirement| requirement == own.1)
}

/// What the Keychain keeps of a trusted application: its path.
fn data(application: &SecTrustedApplication) -> Result<Vec<u8>> {
    let mut data: *const CFData = null();
    // SAFETY: `data` is a valid place for the call's result.
    let status = unsafe { application.copy_data(NonNull::from(&mut data)) };
    checked(status, "check")?;
    let data = NonNull::new(data.cast_mut()).ok_or_else(|| refused("check", 0))?;
    // SAFETY: on success the call returned retained data.
    Ok(unsafe { CFRetained::from_raw(data) }.to_vec())
}

/// What the Keychain keeps of this program, as an entry trusting it names
/// it.
fn own_application() -> Result<Vec<u8>> {
    let mut application: *mut SecTrustedApplication = null_mut();
    // SAFETY: no path names the calling program; `application` is a valid
    // place for the call's result.
    let status =
        unsafe { SecTrustedApplication::create_from_path(null(), NonNull::from(&mut application)) };
    checked(status, "check")?;
    let application = NonNull::new(application).ok_or_else(|| refused("check", 0))?;
    // SAFETY: on success the call returned a retained application.
    let application = unsafe { CFRetained::from_raw(application) };
    data(&application)
}

/// The partitions a partition entry's description lists.
fn listed(description: &str) -> Vec<String> {
    let bytes: Option<Vec<u8>> = (0..description.len())
        .step_by(2)
        .map(|at| {
            description
                .get(at..at + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect();
    let text = bytes
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    text.split("<string>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</string>"))
        .map(|(partition, _)| partition.to_owned())
        .collect()
}

/// This program's code on disk.
fn own_code() -> Option<CFRetained<SecStaticCode>> {
    let mut code: *mut SecCode = null_mut();
    // SAFETY: `code` is a valid place for the call's result.
    let status = unsafe { SecCode::copy_self(SecCSFlags::empty(), NonNull::from(&mut code)) };
    let code = (status == 0).then_some(NonNull::new(code)).flatten()?;
    // SAFETY: on success the call returned a retained code object.
    let code = unsafe { CFRetained::from_raw(code) };
    let mut on_disk: *const SecStaticCode = null();
    // SAFETY: `on_disk` is a valid place for the call's result.
    let status = unsafe { code.copy_static_code(SecCSFlags::empty(), NonNull::from(&mut on_disk)) };
    let on_disk = (status == 0)
        .then_some(NonNull::new(on_disk.cast_mut()))
        .flatten()?;
    // SAFETY: on success the call returned a retained static code object.
    Some(unsafe { CFRetained::from_raw(on_disk) })
}

/// This program's designated requirement, as text: what an entry that
/// trusts this program, and nothing broader, holds.
pub(super) fn own_requirement() -> Result<String> {
    let on_disk = own_code().ok_or_else(|| refused("check", 0))?;
    let mut requirement: *mut SecRequirement = null_mut();
    // SAFETY: `requirement` is a valid place for the call's result.
    let status = unsafe {
        SecCode::copy_designated_requirement(
            &on_disk,
            SecCSFlags::empty(),
            NonNull::from(&mut requirement),
        )
    };
    checked(status, "check")?;
    let requirement = NonNull::new(requirement).ok_or_else(|| refused("check", 0))?;
    // SAFETY: on success the call returned a retained requirement.
    let requirement = unsafe { CFRetained::from_raw(requirement) };
    text(&requirement)
}

/// `requirement` as text.
fn text(requirement: &SecRequirement) -> Result<String> {
    let mut text: *const CFString = null();
    // SAFETY: `text` is a valid place for the call's result.
    let status = unsafe { requirement.copy_string(SecCSFlags::empty(), NonNull::from(&mut text)) };
    checked(status, "check")?;
    let text = NonNull::new(text.cast_mut()).ok_or_else(|| refused("check", 0))?;
    // SAFETY: on success the call returned a retained string.
    Ok(unsafe { CFRetained::from_raw(text) }.to_string())
}

/// The partition this program's own items get: its team when it has one,
/// otherwise its code hash.
fn own_partition() -> Option<String> {
    let on_disk = own_code()?;
    let mut information: *const CFDictionary = null();
    // SAFETY: `information` is a valid place for the call's result.
    let status = unsafe {
        SecCode::copy_signing_information(
            &on_disk,
            SecCSFlags(kSecCSSigningInformation),
            NonNull::from(&mut information),
        )
    };
    let information = (status == 0)
        .then_some(NonNull::new(information.cast_mut()))
        .flatten()?;
    // SAFETY: on success the call returned a retained dictionary, whose keys
    // are the framework's strings and whose values are CF objects.
    let information: CFRetained<CFDictionary<CFString, CFType>> =
        unsafe { CFRetained::cast_unchecked(CFRetained::from_raw(information)) };
    // SAFETY: the keys are the framework's own constants.
    let (team, unique) = unsafe { (kSecCodeInfoTeamIdentifier, kSecCodeInfoUnique) };
    if let Some(team) = information
        .get(team)
        .and_then(|team| team.downcast::<CFString>().ok())
    {
        return Some(format!("teamid:{team}"));
    }
    let hash = information.get(unique)?.downcast::<CFData>().ok()?;
    let hex: String = hash
        .to_vec()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Some(format!("cdhash:{hex}"))
}

#[cfg(test)]
#[path = "made_tests.rs"]
mod tests;
