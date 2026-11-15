//! Where a Finder alias leads. To every other reader an alias is a plain
//! file, but `open` follows it, so what is handed to a desktop to open is
//! judged where its alias leads too. Resolving never asks the person anything
//! and never mounts a volume.
use objc2::rc::Retained;
use objc2_foundation::{NSString, NSURL, NSURLBookmarkResolutionOptions};
use std::path::{Path, PathBuf};

/// The most aliases one path may lead through before the rest is not followed.
const ALIASES: usize = 8;

/// Every place the alias at `path` leads, in order, when it is one: the
/// alias it leads to first, then where that one leads, and so on. Nothing
/// for a path that is no alias or cannot be resolved; a link at the end of
/// `path` is followed as an alias is.
pub fn leads_to(path: &Path) -> Vec<PathBuf> {
    let mut led = Vec::new();
    let mut at = path.to_path_buf();
    while led.len() < ALIASES {
        let Some(next) = resolved(&at) else {
            break;
        };
        if next == at || led.contains(&next) {
            break;
        }
        led.push(next.clone());
        at = next;
    }
    led
}

/// Where the alias, or link, at `path` leads; `path` itself when it is
/// neither, and none when it cannot be told.
fn resolved(path: &Path) -> Option<PathBuf> {
    let url = file_url(path)?;
    let options =
        NSURLBookmarkResolutionOptions::WithoutUI | NSURLBookmarkResolutionOptions::WithoutMounting;
    let target = NSURL::URLByResolvingAliasFileAtURL_options_error(&url, options).ok()?;
    Some(PathBuf::from(target.path()?.to_string()))
}

fn file_url(path: &Path) -> Option<Retained<NSURL>> {
    Some(NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?)))
}

/// Make a Finder alias at `at` that leads to `target`, which must exist.
///
/// # Panics
///
/// Panics when the alias cannot be made: for tests alone.
#[cfg(any(test, feature = "test-support"))]
pub fn make(target: &Path, at: &Path) {
    use objc2_foundation::NSURLBookmarkCreationOptions;
    let target = file_url(target).expect("a target named in UTF-8");
    let bookmark = target
        .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
            NSURLBookmarkCreationOptions::SuitableForBookmarkFile,
            None,
            None,
        )
        .expect("a bookmark of the target");
    let at = file_url(at).expect("an alias named in UTF-8");
    NSURL::writeBookmarkData_toURL_options_error(&bookmark, &at, 0).expect("the alias written");
}

#[cfg(test)]
#[path = "alias_tests.rs"]
mod tests;
