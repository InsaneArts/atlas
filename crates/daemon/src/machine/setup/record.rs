//! What the setup keeps of its own work, root-owned: how far publishing got,
//! with the identities it saw, and whether it is making, or made, the second
//! administrator. Each phase is settled from it when asked again.
use super::{Places, io, sync};
use crate::machine::installer::record::Identity;
use atlas_core::{error::Result, files::Token};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The longest record read.
const LONGEST: u64 = 64 * 1024;

/// Where publishing got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Publishing {
    /// The bundle is being made from the staged one.
    Copying,
    /// It is made and verified, about to be placed.
    Placing,
    /// It is in place, and checked there.
    Placed,
}

/// Publishing one bundle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Publish {
    pub(super) change: Token,
    pub(super) phase: Publishing,
    /// What was installed when it began; none when nothing was.
    pub(super) installed: Option<Identity>,
    /// The bundle it made.
    pub(super) new: Option<Identity>,
}

/// The second administrator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Admin {
    pub(super) name: String,
    /// The account once made; none while it is being made.
    pub(super) uid: Option<u32>,
}

/// Everything the setup keeps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Bootstrap {
    pub(super) publish: Option<Publish>,
    pub(super) admin: Option<Admin>,
}

impl Bootstrap {
    fn file(places: &Places) -> PathBuf {
        places.bootstrap().join("record.json")
    }

    /// The record kept in `places`; an empty one when none is.
    pub(super) fn read(places: &Places) -> Result<Self> {
        let file = Self::file(places);
        if !file.exists() {
            return Ok(Self::default());
        }
        let bytes = atlas_core::files::read_own(&file, LONGEST)
            .ok_or_else(|| super::refused("its record cannot be read"))?;
        serde_json::from_slice(&bytes).map_err(|_| super::refused("its record is damaged"))
    }

    /// Keep it in `places`: written whole beside its place, renamed, and its
    /// folder written through.
    pub(super) fn write(&self, places: &Places) -> Result<()> {
        let folder = places.bootstrap();
        std::fs::create_dir_all(&folder).map_err(io)?;
        let file = Self::file(places);
        atlas_core::files::write_atomic(&file, &serde_json::to_vec(self)?, 0o600)?;
        sync(&folder)
    }
}
