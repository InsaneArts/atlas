//! Uninstall's VM phase: every VM this account keeps removed before
//! anything Atlas shares goes. The person allows it in Atlas.app first,
//! told every VM it removes by name; then the account's uninstall barrier
//! is taken with the grant's change and those VMs, before anything is
//! looked at, so no VM is added or removed meanwhile. Every VM the settings
//! list, kept on disk, or only in a lifecycle record or a name's
//! reservation (an add never published) is removed as a desktop removed on
//! its own is, the settings left as they are, under the uninstall's change
//! unless a removal of it is kept or recorded already; one the person was
//! not told of is never removed. While a removal kept a path or left a
//! Keychain item, nothing more is removed and the barrier stays: asked
//! again, the same uninstall goes on with no new grant, each item looked at
//! again. Once nothing of any VM is left the phase commits, and from then
//! the barrier blocks nothing. The person may call it off instead
//! ([`cancel`]): nothing more is removed, and the barrier blocks nothing.
use super::{Means, Removing, Target, begun, real::Real};
use crate::vm::{
    StationHold,
    add::settings_at,
    lifecycle::{
        Lifecycle,
        barrier::{Barrier, Closed, Phase, Uninstalling},
        grant,
    },
    removal,
};
use atlas_core::{
    config::{
        desktops::{Chosen, Settings},
        write::Disposition,
    },
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::Token,
};
use serde_json::{Value, json};
use std::path::Path;

/// The VM an uninstall's grant names: it is for every VM, named in its
/// name, so it names none of them by identifier.
const EVERY_VM: &str = "000000000000000000000000";

/// What uninstall's VM phase has to do, looked at with nothing made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    /// No VM is listed, kept or recorded here: nothing to remove.
    Nothing,
    /// VMs to remove: the person is asked first.
    Ask,
    /// An uninstall the person allowed was cut short in its VM phase: it
    /// goes on as that very uninstall, its change, with no new grant.
    Resume(Token),
}

/// What uninstall's VM phase has to do for the settings file `settings`,
/// nothing made or changed.
///
/// # Errors
///
/// Fails when the VMs, their lifecycle records or the uninstall barrier
/// cannot be read: VMs may be kept there.
pub fn pending(settings: &Path) -> Result<Pending> {
    pending_in(&Lifecycle::own()?, settings)
}

fn pending_in(lifecycle: &Lifecycle, settings: &Path) -> Result<Pending> {
    if let Some(barrier) = cut_short(lifecycle)? {
        return Ok(Pending::Resume(barrier.change));
    }
    let settings = atlas_core::config::resolve_path(settings)?;
    Ok(if inventory(lifecycle, &settings)?.is_empty() {
        Pending::Nothing
    } else {
        Pending::Ask
    })
}

/// Remove every VM this account keeps, once the person allows it in
/// Atlas.app, or go on with the uninstall whose VM phase was cut short,
/// saying each step to `say`: what each removal came to, with the
/// exclusion that keeps every add and removal out until the uninstall
/// ends. None when there was nothing to remove: the person was not asked
/// and nothing was made.
///
/// # Errors
///
/// Refuses without the person's grant, while a VM is being added or
/// removed, when the uninstall it would go on with was called off or ended
/// meanwhile, and when a VM the person was not told of is found, before
/// anything is removed; fails when a removal fails or is still under way,
/// and refuses, nothing more removed, while a removal kept a path or left a
/// Keychain item. The VM phase then stays under way, to be gone on with by
/// uninstall asked again, or called off.
pub fn remove_every(settings: &Path, say: &dyn Fn(&str)) -> Result<Option<Removed>> {
    remove_every_with(&Lifecycle::own()?, &Real, settings, say)
}

/// Every VM removed, and the exclusion that keeps any other out until
/// the uninstall ends.
#[derive(Debug)]
pub struct Removed {
    /// What each removal came to.
    pub vms: Vec<Value>,
    /// Held while what Atlas shares goes; its end is the uninstall's.
    pub closed: Closed,
}

fn remove_every_with(
    lifecycle: &Lifecycle,
    means: &impl Means,
    settings: &Path,
    say: &dyn Fn(&str),
) -> Result<Option<Removed>> {
    let settings = atlas_core::config::resolve_path(settings)?;
    let (uninstalling, fresh) = match cut_short(lifecycle)? {
        Some(barrier) => {
            say("Going on uninstalling Atlas and its VMs, as you allowed in Atlas.app.");
            (lifecycle.resuming(&barrier.change)?, false)
        }
        None => {
            let told = inventory(lifecycle, &settings)?;
            if told.is_empty() {
                return Ok(None);
            }
            say(&format!(
                "Asking you in Atlas.app to allow uninstalling Atlas, removing {}.",
                names(&told)
            ));
            let named: Vec<String> = told.iter().map(|target| target.name.clone()).collect();
            let change = means.grant(&operation(&settings, &named, false)?)?;
            let ids: Vec<DesktopId> = told.into_iter().map(|target| target.id).collect();
            (lifecycle.uninstalling(&change, &ids)?, true)
        }
    };
    // Looked at again under the barrier: nothing is added or removed now.
    let targets = inventory(lifecycle, &settings)?;
    let untold: Vec<&Target> = targets
        .iter()
        .filter(|target| !uninstalling.vms().contains(&target.id))
        .collect();
    if !untold.is_empty() {
        let others = names(untold);
        // Nothing was removed yet only when this very run was allowed.
        return Err(if fresh {
            uninstalling.call_off()?;
            Error::new(
                Code::State,
                format!(
                    "The VMs changed while you were asked ({others} too), so nothing was removed. \
                     Run atlas uninstall again."
                ),
            )
        } else {
            Error::new(
                Code::State,
                format!(
                    "Atlas also keeps {others}, which you did not allow this uninstall to remove, \
                     so it stopped. Run atlas uninstall --cancel, then atlas uninstall again."
                ),
            )
        });
    }
    let mut removed = Vec::new();
    for target in targets {
        let hold = means.hold(&target.id, &uninstalling)?.ok_or_else(|| {
            Error::new(
                Code::Busy,
                format!(
                    "Another station operation holds the VM \u{201c}{}\u{201d}; run atlas \
                     uninstall again once it is done.",
                    target.name
                ),
            )
        })?;
        removed.push(removed_for_uninstall(
            lifecycle,
            means,
            &uninstalling,
            &hold,
            &settings,
            &target,
            say,
        )?);
    }
    if let Some(blocked) = blocked(&removed, say) {
        return Err(blocked);
    }
    Ok(Some(Removed {
        vms: removed,
        closed: uninstalling.commit()?,
    }))
}

/// Call off the uninstall whose VM phase is under way, once the person
/// allows it in Atlas.app, saying each step to `say`: nothing more is
/// removed, the VMs removed stay removed and the others as they are, and
/// from then the barrier blocks nothing.
///
/// # Errors
///
/// Refuses when no uninstall is under way, without the person's grant, and
/// while an uninstall runs; fails when the barrier cannot be read or kept.
pub fn cancel(settings: &Path, say: &dyn Fn(&str)) -> Result<Value> {
    cancel_with(&Lifecycle::own()?, &Real, settings, say)
}

fn cancel_with(
    lifecycle: &Lifecycle,
    means: &impl Means,
    settings: &Path,
    say: &dyn Fn(&str),
) -> Result<Value> {
    let settings = atlas_core::config::resolve_path(settings)?;
    let nothing = || Error::new(Code::State, "No uninstall is under way to call off.");
    let Some(barrier) = cut_short(lifecycle)? else {
        return Err(nothing());
    };
    // Named as they are found when they can be; a record that cannot be
    // read never keeps the uninstall from being called off, the VMs it
    // was allowed then named by their identifiers.
    let kept: Vec<String> = match inventory(lifecycle, &settings) {
        Ok(left) => left.into_iter().map(|target| target.name).collect(),
        Err(_) => barrier
            .vms
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
    };
    say("Asking you in Atlas.app to allow calling the uninstall off.");
    means.grant(&operation(&settings, &kept, true)?)?;
    if !lifecycle.call_off(&barrier.change)? {
        return Err(nothing());
    }
    Ok(json!({"ok": true, "cancelled": true, "change": barrier.change, "kept": kept}))
}

/// The barrier of the uninstall whose VM phase is under way; none when
/// none is, and nothing made to find out.
fn cut_short(lifecycle: &Lifecycle) -> Result<Option<Barrier>> {
    if !lifecycle.kept_any()? {
        return Ok(None);
    }
    Ok(lifecycle
        .barrier()?
        .filter(|barrier| barrier.phase == Phase::Vms))
}

/// The VMs `targets`, named as the person reads them: "“work” and “spare”".
fn names<'a>(targets: impl IntoIterator<Item = &'a Target>) -> String {
    atlas_core::name::listed(
        &targets
            .into_iter()
            .map(|target| format!("\u{201c}{}\u{201d}", target.name))
            .collect::<Vec<_>>(),
    )
}

/// What the person allows, from the settings file `settings`: uninstalling
/// Atlas with the VMs `names`, named in it, or, when `cancel`, calling the
/// uninstall under way off, the VMs left named.
fn operation(settings: &Path, names: &[String], cancel: bool) -> Result<grant::Operation> {
    let vm = DesktopId::parse(EVERY_VM)
        .ok_or_else(|| Error::new(Code::State, "Uninstall names no desktop."))?;
    Ok(grant::Operation {
        kind: grant::Kind::Uninstall,
        vm,
        name: atlas_core::name::listed(names),
        settings: settings.to_owned(),
        cancel,
    })
}

/// Every VM uninstall removes, as it finds them: listed in the settings at
/// `settings`, kept on disk, with a lifecycle record or a reserved name,
/// or with a removal recorded that may have left something. Each is
/// reconciled by its identifier; one whose removal is recorded finished,
/// with nothing of it left anywhere, is gone.
fn inventory(lifecycle: &Lifecycle, settings: &Path) -> Result<Vec<Target>> {
    let vms = lifecycle.vms();
    // Settings that do not load list nothing here: a VM made here is found
    // on disk or by its record whatever they say.
    let listed: Vec<(String, DesktopId)> = settings_at(settings)
        .ok()
        .flatten()
        .iter()
        .flat_map(Settings::desktops)
        .filter_map(|listed| match &listed.desktop {
            Chosen::Vm(config) => Some((listed.name.clone(), config.id.clone())),
            _ => None,
        })
        .collect();
    let (records, reserved) = if lifecycle.kept_any()? {
        (lifecycle.records()?, lifecycle.reserved_all()?)
    } else {
        (Vec::new(), Vec::new())
    };
    let kept = vms.kept()?;
    let mut ids: Vec<DesktopId> = Vec::new();
    for id in listed
        .iter()
        .map(|(_, id)| id.clone())
        .chain(records.iter().map(|record| record.id.clone()))
        .chain(reserved.iter().map(|(_, id)| id.clone()))
        .chain(kept.iter().cloned())
        .chain(removal::every_recorded(&vms)?)
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let mut targets = Vec::new();
    for id in ids {
        let record = records.iter().find(|record| record.id == id);
        let reservation = reserved
            .iter()
            .find(|(_, reserved)| *reserved == id)
            .map(|(name, _)| name.clone());
        let listed = listed
            .iter()
            .find(|(_, listed)| *listed == id)
            .map(|(name, _)| name.clone());
        let gone = record.is_none()
            && reservation.is_none()
            && !kept.contains(&id)
            && removal::recorded(&vms, &id)?.is_some_and(|recorded| recorded.finished());
        if gone {
            continue;
        }
        let name = listed
            .clone()
            .or_else(|| record.map(|record| record.name.clone()))
            .or(reservation)
            .unwrap_or_else(|| id.as_str().to_owned());
        targets.push(Target {
            named: record.is_none_or(|record| record.named),
            id,
            name,
            listed,
        });
    }
    Ok(targets)
}

/// The VM `target` removed for the uninstall `uninstalling`, under `hold`,
/// as a desktop removed on its own is but with the settings left as they
/// are: what it came to.
fn removed_for_uninstall(
    lifecycle: &Lifecycle,
    means: &impl Means,
    uninstalling: &Uninstalling,
    hold: &StationHold<'_>,
    settings: &Path,
    target: &Target,
    say: &dyn Fn(&str),
) -> Result<Value> {
    let (record, change) = begun(
        lifecycle,
        means,
        target,
        settings,
        uninstalling.change().clone(),
        Disposition::Uninstall,
    )?;
    Removing {
        lifecycle,
        means,
        hold,
        target,
        record,
        disposition: Disposition::Uninstall,
        say,
    }
    .run(&change)
}

/// Why nothing more may be uninstalled while a removal in `removed` kept a
/// path or left a Keychain item, each said to `say`; none when none did.
fn blocked(removed: &[Value], say: &dyn Fn(&str)) -> Option<Error> {
    let mut kept = Vec::new();
    let mut left = Vec::new();
    for done in removed {
        let vm = done["removed"]["name"].as_str().unwrap_or("a VM");
        for path in done["kept"].as_array().into_iter().flatten() {
            let path = path.as_str().unwrap_or_default();
            say(&format!(
                "Atlas kept {path}, found in the VM \u{201c}{vm}\u{201d}'s place: it was not the \
                 VM's own when its removal began, so Atlas never deletes it."
            ));
            kept.push(json!({"vm": vm, "path": path}));
        }
        for item in done["left"].as_array().into_iter().flatten() {
            let (service, account) = (
                item["service"].as_str().unwrap_or_default(),
                item["account"].as_str().unwrap_or_default(),
            );
            say(&format!(
                "The Keychain item {service} {account} of the VM \u{201c}{vm}\u{201d} is left: \
                 Atlas could not delete it without asking."
            ));
            left.push(json!({"vm": vm, "service": service, "account": account}));
        }
    }
    if kept.is_empty() && left.is_empty() {
        return None;
    }
    Some(
        Error::new(
            Code::Uninstall,
            "Atlas stopped before removing anything it shares: what its VMs' removals kept or \
             left is still there.",
        )
        .with(
            "next",
            json!(
                "Move each kept path aside or delete it yourself, delete each Keychain item in \
                 Keychain Access (search for it by its name), then run atlas uninstall again. \
                 Until then no VM is added or removed."
            ),
        )
        .with("kept", json!(kept))
        .with("left", json!(left)),
    )
}

#[cfg(test)]
#[path = "uninstall_tests.rs"]
mod tests;
