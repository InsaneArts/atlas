//! Release tooling. `cargo xtask dist` builds Atlas.app for both Mac
//! architectures, signs it inside out with a Developer ID, checks it against
//! the same requirement setup enforces, optionally notarizes and staples it,
//! and writes a zip with its SHA-256 sum.
use atlas::{
    menubar::{APP_EXECUTABLE, LOGIN_JOB},
    setup::IDENTIFIER,
};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

/// The oldest macOS the app runs on, for every slice and the Info.plist.
const MINIMUM_MACOS: &str = "14.0";
/// Rust triple and architecture for one universal build.
const TARGETS: [(&str, &str); 2] = [
    ("aarch64-apple-darwin", "arm64"),
    ("x86_64-apple-darwin", "x86_64"),
];
/// Beside an app this tool published, so it never replaces anything else.
const MARKER: &str = ".atlas-dist";
const USAGE: &str = "usage: cargo xtask dist --sign IDENTITY [--notarize PROFILE] [--out DIR]

  --sign IDENTITY     Developer ID Application identity to sign with
  --notarize PROFILE  notarytool keychain profile; notarize and staple
  --out DIR           output directory (default: target/dist)";

type Result<T> = std::result::Result<T, String>;

#[derive(Debug, PartialEq)]
struct Options {
    sign: String,
    notarize: Option<String>,
    out: PathBuf,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<()> {
    match args.split_first() {
        Some((command, rest)) if command == "dist" => dist(&options(rest, &root())?),
        _ => Err(USAGE.to_owned()),
    }
}

fn options(args: &[String], root: &Path) -> Result<Options> {
    let mut sign = None;
    let mut notarize = None;
    let mut out = root.join("target/dist");
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--sign" => sign = Some(value()?),
            "--notarize" => notarize = Some(value()?),
            "--out" => out = PathBuf::from(value()?),
            _ => return Err(format!("unknown argument {flag}\n\n{USAGE}")),
        }
    }
    Ok(Options {
        sign: sign.ok_or_else(|| format!("--sign is required\n\n{USAGE}"))?,
        notarize,
        out,
    })
}

/// The repository root (this crate lives in `xtask/`).
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the repository")
        .to_owned()
}

/// Run a command, failing with its name if it does not succeed. A deployment
/// target inherited from the caller's shell is dropped: it could raise the
/// required macOS above the advertised one (and setting it also changes the
/// host's proc-macro builds).
fn exec(program: &str, args: &[&str]) -> Result<()> {
    println!("+ {program} {}", args.join(" "));
    let status = Command::new(program)
        .env_remove("MACOSX_DEPLOYMENT_TARGET")
        .args(args)
        .status()
        .map_err(|error| format!("{program}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{program} failed ({status})"))
}

/// A command's standard output.
fn output(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| format!("{} is not UTF-8", path.display()))
}

/// The app's Info.plist for `version`.
fn info_plist(version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>{IDENTIFIER}</string>
	<key>CFBundleName</key>
	<string>Atlas</string>
	<key>CFBundleDisplayName</key>
	<string>Atlas</string>
	<key>CFBundleExecutable</key>
	<string>atlas</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSMinimumSystemVersion</key>
	<string>{MINIMUM_MACOS}</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHumanReadableCopyright</key>
	<string>InsaneArts</string>
</dict>
</plist>
"#
    )
}

/// No slice of `binary` may require a newer macOS than the advertised one.
fn check_minimum(binary: &Path) -> Result<()> {
    for (_, arch) in TARGETS {
        let build = output(
            "/usr/bin/xcrun",
            &["vtool", "-arch", arch, "-show-build", text(binary)?],
        )?;
        // LC_BUILD_VERSION reports `minos`; the older LC_VERSION_MIN_MACOSX
        // (low deployment targets) reports `version`.
        let minimum = build
            .lines()
            .find_map(|line| {
                let line = line.trim();
                line.strip_prefix("minos ")
                    .or_else(|| line.strip_prefix("version "))
            })
            .map(str::trim);
        if minimum.is_none_or(|minimum| version(minimum) > version(MINIMUM_MACOS)) {
            return Err(format!(
                "{} ({arch}) requires macOS {}, newer than {MINIMUM_MACOS}",
                binary.display(),
                minimum.unwrap_or("?")
            ));
        }
    }
    Ok(())
}

/// A dotted version as comparable numbers.
fn version(text: &str) -> Vec<u32> {
    text.split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// The menu bar's LaunchAgent: `atlas menubar` from inside the app,
/// started at login once the person turns on "Open at Login". Quitting it
/// keeps it quit until the next login.
fn menubar_agent() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LOGIN_JOB}</string>
	<key>BundleProgram</key>
	<string>Contents/MacOS/atlas</string>
	<key>ProgramArguments</key>
	<array>
		<string>atlas</string>
		<string>menubar</string>
	</array>
	<key>AssociatedBundleIdentifiers</key>
	<string>{IDENTIFIER}</string>
	<key>RunAtLoad</key>
	<true/>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#
    )
}

/// Merge per-architecture builds of one executable into a universal one.
fn universal(parts: &[PathBuf], output: &Path) -> Result<()> {
    let mut args = vec!["-create", "-output", text(output)?];
    for part in parts {
        args.push(text(part)?);
    }
    exec("/usr/bin/lipo", &args)?;
    check_universal(output)
}

/// `output` holds every architecture and requires no newer macOS than advertised.
fn check_universal(output: &Path) -> Result<()> {
    let archs = self::output("/usr/bin/lipo", &["-archs", text(output)?])?;
    let found: Vec<&str> = archs.split_whitespace().collect();
    if !(found.contains(&"arm64") && found.contains(&"x86_64")) {
        return Err(format!("{} is not universal: {archs}", output.display()));
    }
    check_minimum(output)
}

/// Refuse output directories holding apps this tool did not build there.
fn publishable(out: &Path) -> Result<()> {
    let home = env::var("HOME").unwrap_or_default();
    let resolved = fs::canonicalize(out).unwrap_or_else(|_| out.to_owned());
    if resolved == Path::new("/Applications") || resolved == Path::new(&home).join("Applications") {
        return Err("--out must be a build directory, not an Applications folder".into());
    }
    if out.join("Atlas.app").exists() && !out.join(MARKER).exists() {
        return Err(format!(
            "{} holds an Atlas.app this tool did not build; choose another --out",
            out.display()
        ));
    }
    Ok(())
}

/// Exchange two existing paths in one atomic step.
#[allow(unsafe_code)] // FFI: renamex_np(RENAME_SWAP) has no safe binding.
fn exchange(first: &Path, second: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = |path: &Path| CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string());
    let (first, second) = (path(first)?, path(second)?);
    // SAFETY: two NUL-terminated paths; RENAME_SWAP exchanges them or fails
    // leaving both in place.
    if unsafe { libc::renamex_np(first.as_ptr(), second.as_ptr(), libc::RENAME_SWAP) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

/// Put the verified `staged` app at `out/Atlas.app`. The marker is
/// written first and the previous app is exchanged atomically, so an
/// interruption never leaves the output without an app this tool owns.
fn publish(staged: &Path, out: &Path) -> Result<PathBuf> {
    let app = out.join("Atlas.app");
    fs::write(out.join(MARKER), "Built by cargo xtask dist\n")
        .map_err(|error| error.to_string())?;
    if app.exists() {
        exchange(staged, &app)?;
        // `staged` now holds the previous build; failing to remove it is harmless.
        let _ = fs::remove_dir_all(staged);
    } else {
        fs::rename(staged, &app).map_err(|error| error.to_string())?;
    }
    Ok(app)
}

fn dist(options: &Options) -> Result<()> {
    let version = atlas::VERSION;
    publishable(&options.out)?;
    // A fresh staging directory per run; nothing is published until verified.
    let stage = options.out.join(format!("stage/{}", std::process::id()));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&stage).map_err(|error| error.to_string())?;
    let upload = options.out.join("notarize.zip");
    // Verified (and notarized when asked): only now replace the previous build.
    let published = build(options, &stage, &upload).and_then(|app| publish(&app, &options.out));
    // The upload and the staging directory go whether or not it worked.
    let _ = fs::remove_file(&upload);
    let _ = fs::remove_dir_all(&stage);
    let app = published?;
    let name = format!("Atlas-{version}-macos.zip");
    let zip = options.out.join(&name);
    let sums_path = options.out.join("SHA256SUMS");
    // Never leave sums that describe an earlier zip.
    let _ = fs::remove_file(&sums_path);
    let _ = fs::remove_file(&zip);
    exec(
        "/usr/bin/ditto",
        &["-c", "-k", "--keepParent", text(&app)?, text(&zip)?],
    )?;
    let sums = format!("{}  {name}\n", sha256(&zip)?);
    fs::write(&sums_path, &sums).map_err(|error| error.to_string())?;
    println!("\n{}\n{sums}", zip.display());
    if options.notarize.is_none() {
        println!("Not notarized: pass --notarize PROFILE for a release.");
    }
    Ok(())
}

/// The SHA-256 of the file at `path`, read in pieces.
fn sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Build, sign and verify Atlas.app in `stage` (notarizing it through
/// `upload` when asked): the app, ready to publish.
fn build(options: &Options, stage: &Path, upload: &Path) -> Result<PathBuf> {
    let root = root();
    let version = atlas::VERSION;
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    // Exactly this package's binary, into a target directory of our own.
    let manifest = root.join("Cargo.toml");
    let target_dir = root.join("target/xtask");
    let mut cli = Vec::new();
    for (rust, _) in TARGETS {
        exec(
            &cargo,
            &[
                "build",
                "--manifest-path",
                text(&manifest)?,
                "--package",
                "atlas",
                "--bin",
                "atlas",
                "--release",
                "--locked",
                "--target",
                rust,
                "--target-dir",
                text(&target_dir)?,
            ],
        )?;
        cli.push(target_dir.join(format!("{rust}/release/atlas")));
    }
    // The Swift app package, for both architectures in one build.
    let package = root.join("app");
    let mut swift = vec!["build", "-c", "release", "--package-path", text(&package)?];
    for (_, arch) in TARGETS {
        swift.extend(["--arch", arch]);
    }
    exec(
        "/usr/bin/swift",
        &[swift.as_slice(), &["--product", APP_EXECUTABLE]].concat(),
    )?;
    let bin = output(
        "/usr/bin/swift",
        &[swift.as_slice(), &["--show-bin-path"]].concat(),
    )?;
    let executable = PathBuf::from(bin.trim()).join(APP_EXECUTABLE);
    let app = stage.join("Atlas.app");
    let macos = app.join("Contents/MacOS");
    fs::create_dir_all(&macos).map_err(|error| error.to_string())?;
    fs::create_dir_all(app.join("Contents/Resources")).map_err(|error| error.to_string())?;
    fs::write(app.join("Contents/Info.plist"), info_plist(version))
        .map_err(|error| error.to_string())?;
    universal(&cli, &macos.join("atlas"))?;
    fs::copy(&executable, macos.join(APP_EXECUTABLE)).map_err(|error| error.to_string())?;
    check_universal(&macos.join(APP_EXECUTABLE))?;
    // The app icon and the helper account's picture, both drawn by
    // app/Resources/AppIcon.swift.
    for file in ["AppIcon.icns", "HelperPicture.png"] {
        fs::copy(
            package.join("Resources").join(file),
            app.join("Contents/Resources").join(file),
        )
        .map_err(|error| error.to_string())?;
    }
    // The agent plugin (Claude Code and Codex), matching this CLI's version.
    exec(
        "/usr/bin/ditto",
        &[
            text(&root.join("agents"))?,
            text(&app.join("Contents/Resources/agents"))?,
        ],
    )?;
    // "Open at login": the menu bar registers this agent with SMAppService.
    let agents = app.join("Contents/Library/LaunchAgents");
    fs::create_dir_all(&agents).map_err(|error| error.to_string())?;
    fs::write(agents.join(format!("{LOGIN_JOB}.plist")), menubar_agent())
        .map_err(|error| error.to_string())?;

    // Inside out: the Swift executable carries its own identifier, then the app
    // (whose main executable is the consented `atlas`) seals it.
    let sign = |identifier: Option<&str>, path: &Path| -> Result<()> {
        let mut args = vec!["--force", "--options", "runtime", "--timestamp"];
        if let Some(identifier) = identifier {
            args.extend(["--identifier", identifier]);
        }
        args.extend(["--sign", &options.sign, text(path)?]);
        exec("/usr/bin/codesign", &args)
    };
    sign(
        Some(atlas::setup::APP_IDENTIFIER),
        &macos.join(APP_EXECUTABLE),
    )?;
    sign(None, &app)?;
    exec(
        "/usr/bin/codesign",
        &["--verify", "--strict", "--deep", text(&app)?],
    )?;
    if !atlas::setup::developer_id(&macos.join("atlas")) {
        return Err(
            "the app does not satisfy setup's Developer ID requirement (identity and team)".into(),
        );
    }

    if let Some(profile) = &options.notarize {
        exec(
            "/usr/bin/ditto",
            &["-c", "-k", "--keepParent", text(&app)?, text(upload)?],
        )?;
        exec(
            "/usr/bin/xcrun",
            &[
                "notarytool",
                "submit",
                text(upload)?,
                "--keychain-profile",
                profile,
                "--wait",
            ],
        )?;
        exec("/usr/bin/xcrun", &["stapler", "staple", text(&app)?])?;
        exec(
            "/usr/sbin/spctl",
            &["--assess", "--type", "execute", "-vv", text(&app)?],
        )?;
    }
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for expected in [
            r#""CFBundleIdentifier":"com.insanearts.atlas""#,
            r#""CFBundleExecutable":"atlas""#,
            r#""CFBundleIconFile":"AppIcon""#,
            r#""CFBundleShortVersionString":"1.2.3""#,
            r#""LSUIElement":true"#,
            r#""LSMinimumSystemVersion":"14.0""#,
        ] {
            assert!(json.contains(expected), "{expected} in {json}");
        }
    }
}
