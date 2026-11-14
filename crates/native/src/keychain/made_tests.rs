use super::*;

#[test]
fn a_partition_list_is_read_from_its_hexadecimal_property_list() {
    let plist = "<?xml version=\"1.0\"?><plist><dict><key>Partitions</key>\
        <array><string>teamid:ABCDE12345</string><string>apple-tool:</string></array>\
        </dict></plist>";
    let hex: String = plist.bytes().map(|byte| format!("{byte:02x}")).collect();

    assert_eq!(listed(&hex), ["teamid:ABCDE12345", "apple-tool:"]);
}

#[test]
fn a_description_that_is_not_hexadecimal_lists_nothing() {
    assert!(listed("not hex at all").is_empty());
}

#[test]
fn this_program_has_a_partition_of_its_own() {
    let own = own_partition().unwrap();

    assert!(own.starts_with("teamid:") || own.starts_with("cdhash:"));
}

/// A trusted application entry for the program at `path`; this program's
/// own with none.
#[expect(
    deprecated,
    reason = "trusted applications exist only in the file Keychain API"
)]
fn trusted(path: Option<&str>) -> CFRetained<SecTrustedApplication> {
    let path = path.map(|path| std::ffi::CString::new(path).unwrap());
    let mut application: *mut SecTrustedApplication = null_mut();
    // SAFETY: the path is a valid C string or none; `application` is a
    // valid place for the call's result.
    let status = unsafe {
        SecTrustedApplication::create_from_path(
            path.as_ref().map_or(null(), |path| path.as_ptr()),
            NonNull::from(&mut application),
        )
    };
    assert_eq!(status, 0);
    // SAFETY: on success the call returned a retained application.
    unsafe { CFRetained::from_raw(NonNull::new(application).unwrap()) }
}

#[test]
fn an_entry_is_this_program_only_with_its_path_and_its_own_requirement() {
    let own = (own_application().unwrap(), own_requirement().unwrap());

    assert!(is_this(&trusted(None), &own));
    assert!(!is_this(&trusted(Some("/bin/ls")), &own));
}
