use super::*;
use crate::handoff::Copied;
use atlas_core::files::Token;
use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

#[test]
fn a_failed_launch_keeps_copied_paths_or_explicit_null_for_non_unicode_paths() {
    let finished = Finished::decode(&json!({"timedOut": true}), OUTPUT);
    for invalid in [None, Some("from"), Some("to")] {
        let mut copied = vec![Copied {
            from: PathBuf::from("/original/note.txt"),
            to: PathBuf::from("/copies/note.txt"),
            token: Token::new(),
        }];
        let expected = match invalid {
            None => json!([{"from": "/original/note.txt", "to": "/copies/note.txt"}]),
            Some(field) => {
                let path = PathBuf::from(OsString::from_vec(b"/note-\xff.txt".to_vec()));
                if field == "from" {
                    copied[0].from = path;
                } else {
                    copied[0].to = path;
                }
                Value::Null
            }
        };
        let error = not_opened(&finished, &copied);
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"ok": false, "code": "wait-failed", "uncertain": false,
                "error": "macOS did not finish opening that in time; it may still be launching: \
                              run atlas wait --app NAME, or look with atlas apps",
                "timedOut": true, "opened": false, "copied": expected})
        );
    }
    assert!(
        serde_json::to_value(not_opened(&finished, &[]))
            .unwrap()
            .get("copied")
            .is_none()
    );
}
