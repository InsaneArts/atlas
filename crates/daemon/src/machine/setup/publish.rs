//! Publishing the Atlas the guest account unpacked: made again by root with
//! the root installer's own copy, from a stage snapshot of it, so nothing
//! is ever extracted as root; verified; then placed by one exclusive rename,
//! or one swap with an Atlas already there of other code, which is kept.
use super::{
    Places, System, io,
    record::{Bootstrap, Publish, Publishing},
    refused, sync,
};
use crate::machine::installer::{
    copy::{BUNDLE, copy_in},
    record::Identity,
};
use atlas_core::{
    error::Result,
    files::Token,
    fs_move::{FileName, Moved, exchange_between, move_between},
    stage::Snapshot,
};
use serde_json::{Value, json};
use std::{fs::File, path::Path};

fn bundle() -> FileName {
    FileName::new(BUNDLE).expect("a plain name")
}

fn synced(moved: Moved) -> Result<()> {
    match moved {
        Moved::Synced => Ok(()),
        Moved::Unsynced(error) => Err(io(error)),
    }
}

/// Publish the Atlas staged at `source`, for the install change `change`.
pub(super) fn publish(
    places: &Places,
    system: &impl System,
    change: &Token,
    source: &Path,
) -> Result<Value> {
    let mut kept = Bootstrap::read(places)?;
    if let Some(begun) = kept.publish.clone().filter(|begun| &begun.change == change) {
        match begun.phase {
            Publishing::Placed => return Ok(json!({"ok": true, "published": "placed"})),
            Publishing::Placing => return settle_placing(places, system, &mut kept, begun),
            // Cut short while copying: nothing was placed; made again.
            Publishing::Copying => {}
        }
    }
    let manifest = Snapshot::of(source)?;
    let applications = File::open(places.applications()).map_err(io)?;
    let installed = Identity::of(&applications, &bundle())?;
    if installed.is_some_and(|seen| !seen.folder) {
        return Err(refused("something that is not an app is in Atlas's place"));
    }
    kept.publish = Some(Publish {
        change: change.clone(),
        phase: Publishing::Copying,
        installed,
        new: None,
    });
    kept.write(places)?;
    let folder = places.bootstrap().join(change.as_str());
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir(&folder).map_err(io)?;
    std::fs::set_permissions(&folder, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .map_err(io)?;
    let into = File::open(&folder).map_err(io)?;
    copy_in(&manifest, source, &into, &bundle(), system.root())?;
    let Some(cdhash) = system.verified(&folder.join(BUNDLE)) else {
        return Err(refused("the staged Atlas is not Atlas's, signed"));
    };
    // One already there with this very code, and root's own as the machine
    // job needs, is kept as it is.
    let present = places.applications().join(BUNDLE);
    let owned = !system.root()
        || crate::machine::launch::root_owned(&present.join("Contents/MacOS/atlas")).is_ok();
    if installed.is_some() && owned && system.verified(&present).as_ref() == Some(&cdhash) {
        std::fs::remove_dir_all(folder.join(BUNDLE)).map_err(io)?;
        let publishing = kept.publish.as_mut().expect("kept above");
        publishing.phase = Publishing::Placed;
        kept.write(places)?;
        return Ok(json!({"ok": true, "published": "kept"}));
    }
    let new = Identity::of(&into, &bundle())?;
    let publishing = kept.publish.as_mut().expect("kept above");
    publishing.phase = Publishing::Placing;
    publishing.new = new;
    kept.write(places)?;
    match installed {
        None => synced(move_between(&into, &bundle(), &applications, &bundle()).map_err(io)?)?,
        Some(_) => {
            synced(exchange_between(&applications, &bundle(), &into, &bundle()).map_err(io)?)?
        }
    }
    let begun = kept.publish.clone().expect("kept above");
    finish(places, system, &mut kept, &begun, &cdhash)
}

/// Settle a placing cut short, by the identities found: placed, placed
/// again, or refused when something else is there.
fn settle_placing(
    places: &Places,
    system: &impl System,
    kept: &mut Bootstrap,
    begun: Publish,
) -> Result<Value> {
    let applications = File::open(places.applications()).map_err(io)?;
    let into = File::open(places.bootstrap().join(begun.change.as_str())).map_err(io)?;
    let now = Identity::of(&applications, &bundle())?;
    if now != begun.new {
        if now != begun.installed {
            return Err(refused("an Atlas this setup did not place is installed"));
        }
        // Not yet placed: placed now.
        match begun.installed {
            None => synced(move_between(&into, &bundle(), &applications, &bundle()).map_err(io)?)?,
            Some(_) => {
                synced(exchange_between(&applications, &bundle(), &into, &bundle()).map_err(io)?)?
            }
        }
    }
    let Some(cdhash) = system.verified(&places.applications().join(BUNDLE)) else {
        return Err(refused("the placed Atlas is not Atlas's, signed"));
    };
    finish(places, system, kept, &begun, &cdhash)
}

/// Check what was placed: this setup's bundle, Atlas's code, and owned as
/// `machine::launch` requires; then kept placed.
fn finish(
    places: &Places,
    system: &impl System,
    kept: &mut Bootstrap,
    begun: &Publish,
    cdhash: &[u8],
) -> Result<Value> {
    let applications = File::open(places.applications()).map_err(io)?;
    let placed = places.applications().join(BUNDLE);
    if Identity::of(&applications, &bundle())? != begun.new
        || system.verified(&placed).as_deref() != Some(cdhash)
    {
        return Err(refused("the placed Atlas is not the one this setup made"));
    }
    if system.root() {
        crate::machine::launch::root_owned(&placed.join("Contents/MacOS/atlas"))?;
    }
    sync(&places.applications())?;
    if let Some(publishing) = kept.publish.as_mut() {
        publishing.phase = Publishing::Placed;
    }
    kept.write(places)?;
    Ok(json!({"ok": true, "published": "placed"}))
}
