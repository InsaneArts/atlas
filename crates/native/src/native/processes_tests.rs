use super::*;

#[test]
fn a_snapshot_never_assigns_a_replacement_identity_to_an_earlier_row() {
    for now in [None, Some([11, 2])] {
        let (rows, starts) = snapshot_with(
            vec![Instance::new(812, [10, 2])],
            || Ok(vec![812, 812, 900]),
            |pid| *pid,
            |pid| {
                assert_eq!(pid, 812, "new processes are never adopted");
                Ok(now)
            },
        )
        .unwrap();
        assert_eq!(rows, [812, 812, 900]);
        assert!(starts.is_empty());
    }
}

#[test]
fn several_windows_share_one_checked_start_time_per_process() {
    let mut checks = 0;
    let (rows, starts) = snapshot_with(
        vec![Instance::new(812, [10, 2])],
        || Ok(vec![812, 812]),
        |pid| *pid,
        |pid| {
            assert_eq!(pid, 812);
            checks += 1;
            Ok(Some([10, 2]))
        },
    )
    .unwrap();
    assert_eq!(rows, [812, 812]);
    assert_eq!(starts, BTreeMap::from([(812, [10, 2])]));
    assert_eq!(checks, 1);
}

#[test]
fn a_denied_helper_lookup_refuses_instead_of_returning_unpinned_rows() {
    let error = snapshot_with(
        vec![Instance::new(812, [10, 2])],
        || Ok(vec![812]),
        |pid| *pid,
        |_| Err(std::io::Error::from_raw_os_error(libc::EPERM).into()),
    )
    .unwrap_err();
    assert_eq!(error.code, atlas_core::error::Code::Io);
}
