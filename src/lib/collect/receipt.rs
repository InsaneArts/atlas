//! A collection's receipt: its name, the files it kept with the size and
//! digest each copy must still have, and whether those copies still have
//! them now.
use super::{COLLECTED, UNLABELLED};
use crate::{
    digest::file_sha256,
    host_session::Key,
    name::{lower_hex, name_type, random_hex},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The receipt format this build writes.
pub(super) const VERSION: u32 = 1;

name_type! {
    /// A collection's receipt: when it was made, in milliseconds since
    /// 1970, and six random hexadecimal digits, so two collections never
    /// share one.
    #[derive(Hash)]
    pub struct ReceiptId;
    refused(text) = format!("{text} is not a collection's receipt");
    display
}

impl ReceiptId {
    /// A new receipt, made at `at`.
    pub(super) fn new(at: u64) -> Self {
        Self(format!("{at}-{}", random_hex(6)))
    }

    /// `text` as a receipt, when it is one of Atlas's.
    pub fn parse(text: &str) -> Option<Self> {
        let (at, random) = text.split_once('-')?;
        let at_ok =
            !at.is_empty() && at.len() <= 20 && at.bytes().all(|byte| byte.is_ascii_digit());
        (at_ok && lower_hex(random, 6)).then(|| Self(text.to_owned()))
    }
}

/// One file a collection kept: where it was in the helper, where its copy
/// is, and what the copy must still be.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Kept {
    pub source: String,
    /// The source relative to the helper's home, when it is in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_home: Option<String>,
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
    /// When this version of the source was read, by this Mac's clock;
    /// none in a receipt from before Atlas kept it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<ReadSpan>,
}

/// When one version of a file was read: from its first part to its
/// verified digest, in milliseconds since 1970 by this Mac's clock. The
/// bytes kept are the file as it stood somewhere in between.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadSpan {
    pub started: u64,
    pub finished: u64,
}

impl ReadSpan {
    /// Whether this span can be trusted to order versions, for a collection
    /// made at `collected`: a clock that moved backwards while it was read,
    /// or since the collection began, leaves it out of order.
    pub fn trusted(&self, collected: u64) -> bool {
        collected <= self.started && self.started <= self.finished
    }

    /// Whether this version was read wholly before `other` began: only then
    /// is `other` the newer of the two.
    pub fn before(&self, other: &Self) -> bool {
        self.finished < other.started
    }
}

/// A collection's receipt file, beside its copies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Receipt {
    pub(super) version: u32,
    pub(super) receipt: ReceiptId,
    pub(super) at: u64,
    pub(super) task: Option<Key>,
    pub(super) files: Vec<Kept>,
}

/// How a collection's copies stand now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Intact {
    /// The files whose copies are as they were collected.
    pub kept: Vec<Kept>,
    /// The files whose copies are gone.
    pub missing: Vec<Kept>,
    /// The files whose copies no longer have their size and digest.
    pub changed: Vec<Kept>,
    /// The receipt itself is gone or cannot be read, so which files it
    /// kept is not known.
    pub unknown: bool,
}

impl Intact {
    /// Whether every copy is as it was collected.
    pub(crate) fn whole(&self) -> bool {
        self.missing.is_empty() && self.changed.is_empty() && !self.unknown
    }
}

/// Whether the copies the collection `receipt` of `task` kept in `output`
/// still have their recorded size and digest. A receipt that cannot be
/// read leaves nothing known kept.
pub(crate) fn intact(output: &Path, task: Option<&Key>, receipt: &ReceiptId) -> Intact {
    let owner = task.map_or_else(|| UNLABELLED.to_owned(), Key::to_string);
    let path = output
        .join(COLLECTED)
        .join(owner)
        .join(receipt.as_str())
        .join("receipt.json");
    let read = crate::files::read_own(&path, 1024 * 1024)
        .and_then(|bytes| serde_json::from_slice::<Receipt>(&bytes).ok())
        .filter(|read| read.receipt == *receipt);
    let Some(read) = read else {
        return Intact {
            unknown: true,
            ..Intact::default()
        };
    };
    let mut intact = Intact::default();
    for file in read.files {
        let size = fs::symlink_metadata(&file.path)
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len());
        match size {
            None => intact.missing.push(file),
            Some(size) if size != file.size => intact.changed.push(file),
            Some(_) if file_sha256(&file.path).as_deref() != Some(file.sha256.as_str()) => {
                intact.changed.push(file);
            }
            Some(_) => intact.kept.push(file),
        }
    }
    intact
}
