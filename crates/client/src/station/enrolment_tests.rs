use super::*;
use std::{collections::BTreeMap, sync::Mutex};

/// Enrolments kept in memory, by VM.
#[derive(Default)]
pub(crate) struct Memory(pub(crate) Mutex<BTreeMap<String, ([u8; KEY_LEN], Token)>>);

impl Enrolments for Memory {
    fn keep(&self, vm: &DesktopId, enrolling: &Enrolling) -> Result<()> {
        self.0.lock().unwrap().insert(
            vm.as_str().to_owned(),
            (*enrolling.secret.as_bytes(), enrolling.change.clone()),
        );
        Ok(())
    }

    fn kept(&self, vm: &DesktopId) -> Result<Option<Enrolling>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(vm.as_str())
            .map(|(secret, change)| Enrolling {
                change: change.clone(),
                secret: PairingSecret::from_bytes(*secret),
            }))
    }

    fn forget(&self, vm: &DesktopId) -> Result<bool> {
        Ok(self.0.lock().unwrap().remove(vm.as_str()).is_some())
    }
}

fn vm() -> DesktopId {
    DesktopId::parse("0123456789abcdef01234567").unwrap()
}

#[test]
fn an_enrolment_given_with_its_secret_on_standard_input_is_kept_and_reads_back() {
    let store = Memory::default();
    let change = Token::new();
    let secret = [0xa7; KEY_LEN];
    let hex = atlas_core::name::hex(&secret);
    let input = format!("{hex}\n");

    let said = enrol(
        &store,
        &format!("vm-{}", vm().as_str()),
        change.as_str(),
        &mut input.as_bytes(),
    )
    .unwrap();

    assert_eq!(said["kept"], true);
    // The secret is never said back.
    assert!(!said.to_string().contains(&hex));
    let kept = store.kept(&vm()).unwrap().unwrap();
    assert_eq!(kept.change, change);
    assert_eq!(kept.secret.as_bytes(), &secret);
    assert!(store.forget(&vm()).unwrap());
    assert!(store.kept(&vm()).unwrap().is_none());
}

#[test]
fn an_enrolment_whose_secret_is_not_on_standard_input_is_refused_and_nothing_kept() {
    let store = Memory::default();
    let key = format!("vm-{}", vm().as_str());
    let change = Token::new();

    for given in ["", "abc", &"z".repeat(64), &"a".repeat(66)] {
        let refused = enrol(&store, &key, change.as_str(), &mut given.as_bytes()).unwrap_err();

        assert_eq!(refused.code, Code::BadRequest, "{given:?}");
    }
    assert!(store.0.lock().unwrap().is_empty());
}

#[test]
fn an_enrolment_for_a_key_that_names_no_vm_or_a_token_that_is_none_is_refused() {
    let store = Memory::default();
    let secret = "b".repeat(64);
    let change = Token::new();

    for (key, token) in [
        ("user", change.as_str()),
        ("vm-0123", change.as_str()),
        (&format!("vm-{}", vm().as_str())[..], "not a token"),
    ] {
        let refused = enrol(&store, key, token, &mut secret.as_bytes()).unwrap_err();

        assert_eq!(refused.code, Code::BadRequest, "{key} {token}");
    }
    assert!(store.0.lock().unwrap().is_empty());
}

#[test]
fn a_kept_secret_is_written_as_64_hexadecimal_digits_and_nothing_else_reads_as_one() {
    let secret = [0x0f; KEY_LEN];

    let read = decoded(&atlas_core::name::hex(&secret)).unwrap();

    assert_eq!(read.as_bytes(), &secret);
    assert!(decoded(&"0".repeat(63)).is_none());
    assert!(decoded(&format!("+{}", "0".repeat(63))).is_none());
}

#[test]
#[ignore = "writes to the account's own Keychain; run by hand on a Mac"]
fn an_enrolment_kept_in_the_keychain_reads_back_and_is_deleted() {
    let vm = DesktopId::parse(&format!("{:024x}", std::process::id())).unwrap();
    let enrolling = Enrolling {
        change: Token::new(),
        secret: PairingSecret::from_bytes([3; KEY_LEN]),
    };

    Keychain.keep(&vm, &enrolling).unwrap();

    let kept = Keychain.kept(&vm).unwrap().unwrap();
    assert_eq!(kept.change, enrolling.change);
    assert_eq!(kept.secret.as_bytes(), &[3; KEY_LEN]);
    assert!(Keychain.forget(&vm).unwrap());
    assert!(Keychain.kept(&vm).unwrap().is_none());
}
