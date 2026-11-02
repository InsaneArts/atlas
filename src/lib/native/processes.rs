//! Process identities attached to app and window snapshots inside the helper.
use crate::{
    error::Result,
    process::{self, Instance, Start},
};
use std::collections::BTreeMap;

/// Read rows between process observations. Only a helper process present
/// throughout the read receives an identity. Other rows (including the
/// Window Server's menu bar) remain visible but cannot resolve an input target.
pub(crate) fn snapshot<T>(
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
mod tests;
