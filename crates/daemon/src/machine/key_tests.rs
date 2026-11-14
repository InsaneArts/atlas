use super::*;
use crate::machine::vault::Memory;
use atlas_session::keys::own_key;
use std::sync::Arc;

#[test]
fn the_guest_keeps_one_key_across_loads() {
    let store = GuestKey::new(Arc::new(Memory::default()));

    let first = own_key(&store).unwrap();
    let again = own_key(&store).unwrap();

    assert_eq!(first.public(), again.public());
}

#[test]
fn a_damaged_key_is_refused_never_replaced() {
    let vault = Arc::new(Memory::default());
    vault.damage(NAME, &[7; KEY_LEN - 1]);
    let store = GuestKey::new(vault.clone());

    assert!(own_key(&store).is_err());
    assert_eq!(vault.read(NAME).unwrap().unwrap(), [7; KEY_LEN - 1]);
}
