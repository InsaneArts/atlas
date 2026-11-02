use super::*;
use sha2::{Digest, Sha256};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_owned()).collect()
}

#[test]
fn options_need_an_identity_and_take_a_profile_and_directory() {
    let root = Path::new("/repo");
    assert!(
        options(&[], root)
            .unwrap_err()
            .contains("--sign is required")
    );

    let parsed = options(&args(&["--sign", "Developer ID", "--notarize", "p"]), root).unwrap();
    assert_eq!(
        parsed,
        Options {
            sign: "Developer ID".into(),
            notarize: Some("p".into()),
            out: root.join("target/dist"),
        }
    );

    let out = options(&args(&["--sign", "x", "--out", "/tmp/o"]), root).unwrap();
    assert_eq!(out.out, PathBuf::from("/tmp/o"));

    assert!(
        options(&args(&["--sign"]), root)
            .unwrap_err()
            .contains("needs a value")
    );
    assert!(
        options(&args(&["--bogus"]), root)
            .unwrap_err()
            .contains("unknown")
    );
}

#[test]
fn bundle_permissions_make_fresh_and_cached_artifacts_readable() {
    let directory = tempfile::tempdir().unwrap();
    let app = directory.path().join("Atlas.app");
    let folders = [
        "",
        "Contents",
        "Contents/MacOS",
        "Contents/_CodeSignature",
        "Contents/Resources",
        "Contents/Resources/agents",
    ];
    for folder in folders {
        let path = app.join(folder);
        fs::create_dir_all(&path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let files = [
        ("Contents/MacOS/atlas", 0o700, 0o755),
        ("Contents/MacOS/AtlasApp", 0o700, 0o755),
        ("Contents/Resources/agents/run.sh", 0o700, 0o755),
        ("Contents/Info.plist", 0o600, 0o644),
        ("Contents/_CodeSignature/CodeResources", 0o600, 0o644),
    ];
    for (name, before, _) in files {
        let path = app.join(name);
        fs::write(&path, b"unchanged bundle content").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(before)).unwrap();
    }
    bundle_permissions(&app).unwrap();
    for folder in folders {
        assert_eq!(
            fs::metadata(app.join(folder)).unwrap().permissions().mode() & 0o7777,
            0o755
        );
    }
    for (name, _, after) in files {
        let path = app.join(name);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            after,
            "{name}"
        );
        assert_eq!(fs::read(path).unwrap(), b"unchanged bundle content");
    }
}

#[test]
fn bundle_permissions_refuse_links_without_changing_their_targets() {
    let directory = tempfile::tempdir().unwrap();
    let app = directory.path().join("Atlas.app");
    fs::create_dir(&app).unwrap();
    let target = directory.path().join("not-bundled");
    fs::write(&target, b"private build evidence").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink(&target, app.join("link")).unwrap();
    assert!(bundle_permissions(&app).is_err());
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    assert_eq!(fs::read(target).unwrap(), b"private build evidence");
}

#[test]
fn a_streamed_hash_matches_the_whole_file_hash() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Atlas.zip");
    let bytes: Vec<u8> = (0..200_000).map(|i| (i % 251) as u8).collect();
    fs::write(&path, &bytes).unwrap();

    assert_eq!(
        sha256(&path).unwrap(),
        format!("{:x}", Sha256::digest(&bytes))
    );
    assert!(sha256(&dir.path().join("missing")).is_err());
}

#[test]
fn only_plain_numbered_versions_are_released() {
    for version in ["0.2.0", "1.10.3", atlas_core::VERSION] {
        assert!(release_version(version).is_ok(), "{version}");
    }

    for version in ["0.2.0-beta", "0.2.0+build", "0.2.", "v0.2.0", ""] {
        assert!(release_version(version).is_err(), "{version}");
    }
}

#[test]
fn minimum_versions_compare_numerically() {
    assert!(version("11.0") <= version(MINIMUM_MACOS));
    assert!(version("10.12") <= version(MINIMUM_MACOS));
    assert!(version("15.0") > version(MINIMUM_MACOS));
    assert!(version("14.10") > version("14.9"));
}

#[test]
fn output_directories_never_hold_apps_this_tool_did_not_build() {
    let out = tempfile::tempdir().unwrap();
    let dir = out.path();
    fs::create_dir_all(dir.join("Atlas.app")).unwrap();
    assert!(publishable(dir).unwrap_err().contains("did not build"));

    fs::write(dir.join(MARKER), "").unwrap();
    assert!(publishable(dir).is_ok());
    assert!(publishable(Path::new("/Applications")).is_err());

    let staged = dir.join("stage/1/Atlas.app");
    fs::create_dir_all(&staged).unwrap();
    fs::write(staged.join("new"), "").unwrap();
    let app = publish(&staged, dir).unwrap();
    assert!(app.join("new").exists());
    assert!(!staged.exists(), "the previous build is cleared");

    let again = dir.join("stage/2/Atlas.app");
    fs::create_dir_all(&again).unwrap();
    fs::write(again.join("newer"), "").unwrap();
    let app = publish(&again, dir).unwrap();
    assert!(app.join("newer").exists() && !app.join("new").exists());
}

/// A plist written to a temporary file, as `plutil` reads it back.
fn plist_json(text: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.plist");
    fs::write(&path, text).unwrap();

    output(
        "/usr/bin/plutil",
        &["-convert", "json", "-o", "-", path.to_str().unwrap()],
    )
    .unwrap()
}

#[test]
fn the_login_agent_runs_the_menu_bar_from_inside_the_app() {
    let json = plist_json(&menubar_agent());

    for expected in [
        r#""Label":"com.insanearts.atlas.menubar""#,
        r#""BundleProgram":"Contents\/MacOS\/atlas""#,
        r#""ProgramArguments":["atlas","menubar"]"#,
        r#""RunAtLoad":true"#,
    ] {
        assert!(json.contains(expected), "{expected} in {json}");
    }

    assert!(!json.contains("KeepAlive"), "quitting keeps it quit");
}

#[test]
fn the_info_plist_is_valid_and_names_the_consented_executable() {
    let json = plist_json(&info_plist("1.2.3"));

    assert!(
        root().join("app/Resources/AppIcon.icns").is_file(),
        "the icon the plist names is checked in"
    );
    assert!(
        root().join("app/Resources/HelperPicture.png").is_file(),
        "the helper's account picture is checked in"
    );

    for file in ["icon.json", "Assets/Mark.png"] {
        assert!(
            root()
                .join("app/Resources/AppIcon.icon")
                .join(file)
                .is_file(),
            "the Icon Composer icon the plist names is checked in: {file}"
        );
    }

    for expected in [
        r#""CFBundleIdentifier":"com.insanearts.atlas""#,
        r#""CFBundleExecutable":"atlas""#,
        r#""CFBundleIconFile":"AppIcon""#,
        r#""CFBundleIconName":"AppIcon""#,
        r#""CFBundleShortVersionString":"1.2.3""#,
        r#""LSUIElement":true"#,
        r#""LSMinimumSystemVersion":"14.0""#,
    ] {
        assert!(json.contains(expected), "{expected} in {json}");
    }
}
