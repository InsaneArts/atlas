//! Process identities attached to app and window snapshots inside the helper.
use crate::{
    error::Result,
    process::{self, Instance, Start},
};
use std::collections::BTreeMap;

/// Read rows between process observations. Only a helper process present
/// throughout the read receives an identity. Other rows (including the
/// Window Server's menu bar) remain visible but cannot resolve an input target.
pub fn snapshot<T>(
    read: impl FnOnce() -> Result<Vec<T>>,
    pid: impl Fn(&T) -> i64,
) -> Result<(Vec<T>, BTreeMap<i64, Start>)> {
    let uid = nix::unistd::geteuid().as_raw();
    let before = process::of_user(uid)?
        .into_iter()
        .filter(|info| !info.zombie)
        .map(|info| info.instance)
        .collect();
    snapshot_with(before, read, pid, |pid| {
        Ok(process::info(pid)?
            .filter(|info| info.uid == uid && !info.zombie)
            .map(|info| info.instance.start))
    })
}

fn snapshot_with<T>(
    before: Vec<Instance>,
    read: impl FnOnce() -> Result<Vec<T>>,
    pid: impl Fn(&T) -> i64,
    mut after: impl FnMut(i32) -> Result<Option<Start>>,
) -> Result<(Vec<T>, BTreeMap<i64, Start>)> {
    let before: BTreeMap<_, _> = before
        .into_iter()
        .map(|instance| (i64::from(instance.pid), instance.start))
        .collect();
    let rows = read()?;
    let mut starts = BTreeMap::new();
    for number in rows
        .iter()
        .map(&pid)
        .collect::<std::collections::BTreeSet<_>>()
    {
        if let Some(start) = before.get(&number)
            && after(number as i32)? == Some(*start)
        {
            starts.insert(number, *start);
        }
    }
    Ok((rows, starts))
}

#[cfg(test)]
mod tests {
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
        assert_eq!(error.code, crate::error::Code::Io);
    }
}
