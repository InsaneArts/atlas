//! Release tooling. `cargo xtask dist` builds Understudy.app for both Mac
//! architectures, signs it inside out with a Developer ID, checks it against
//! the same requirement setup enforces, optionally notarizes and staples it,
//! and writes a zip with its SHA-256 sum.
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

/// The bundle identifier; consent follows it with the signing team.
const IDENTIFIER: &str = "com.insanearts.understudy";
const COMPANION: &str = "UnderstudyBar";
/// Rust and Swift targets for one universal build.
const TARGETS: [(&str, &str); 2] = [
    ("aarch64-apple-darwin", "arm64-apple-macos14"),
    ("x86_64-apple-darwin", "x86_64-apple-macos14"),
];
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

/// Run a command, failing with its name if it does not succeed.
fn exec(program: &str, args: &[&str]) -> Result<()> {
    println!("+ {program} {}", args.join(" "));
    let status = Command::new(program)
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
	<string>Understudy</string>
	<key>CFBundleDisplayName</key>
	<string>Understudy</string>
	<key>CFBundleExecutable</key>
	<string>understudy</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSMinimumSystemVersion</key>
	<string>14.0</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHumanReadableCopyright</key>
	<string>InsaneArts</string>
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
    let archs = self::output("/usr/bin/lipo", &["-archs", text(output)?])?;
    let found: Vec<&str> = archs.split_whitespace().collect();
    if !(found.contains(&"arm64") && found.contains(&"x86_64")) {
        return Err(format!("{} is not universal: {archs}", output.display()));
    }
    Ok(())
}

fn dist(options: &Options) -> Result<()> {
    let root = root();
    let version = understudy::VERSION;
    let stage = options.out.join("stage");
    fs::create_dir_all(&stage).map_err(|error| error.to_string())?;
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cli = Vec::new();
    let mut companion = Vec::new();
    for (rust, swift) in TARGETS {
        exec(
            &cargo,
            &["build", "--release", "--locked", "--target", rust],
        )?;
        cli.push(root.join(format!("target/{rust}/release/understudy")));
        let built = stage.join(format!("{COMPANION}-{rust}"));
        exec(
            "/usr/bin/swiftc",
            &[
                "-O",
                "-target",
                swift,
                "-framework",
                "AppKit",
                text(&root.join("menubar/UnderstudyBar.swift"))?,
                "-o",
                text(&built)?,
            ],
        )?;
        companion.push(built);
    }
    let app = options.out.join("Understudy.app");
    if app.exists() {
        fs::remove_dir_all(&app).map_err(|error| error.to_string())?;
    }
    let macos = app.join("Contents/MacOS");
    fs::create_dir_all(&macos).map_err(|error| error.to_string())?;
    fs::create_dir_all(app.join("Contents/Resources")).map_err(|error| error.to_string())?;
    fs::write(app.join("Contents/Info.plist"), info_plist(version))
        .map_err(|error| error.to_string())?;
    universal(&cli, &macos.join("understudy"))?;
    universal(&companion, &macos.join(COMPANION))?;

    // Inside out: the companion carries its own identifier, then the app
    // (whose main executable is the consented `understudy`) seals it.
    let sign = |identifier: Option<&str>, path: &Path| -> Result<()> {
        let mut args = vec!["--force", "--options", "runtime", "--timestamp"];
        if let Some(identifier) = identifier {
            args.extend(["--identifier", identifier]);
        }
        args.extend(["--sign", &options.sign, text(path)?]);
        exec("/usr/bin/codesign", &args)
    };
    sign(Some(&format!("{IDENTIFIER}.bar")), &macos.join(COMPANION))?;
    sign(None, &app)?;
    exec(
        "/usr/bin/codesign",
        &["--verify", "--strict", "--deep", text(&app)?],
    )?;
    if !understudy::setup::developer_id(&macos.join("understudy")) {
        return Err(
            "the app does not satisfy setup's Developer ID requirement (identity and team)".into(),
        );
    }

    if let Some(profile) = &options.notarize {
        let upload = options.out.join("notarize.zip");
        exec(
            "/usr/bin/ditto",
            &["-c", "-k", "--keepParent", text(&app)?, text(&upload)?],
        )?;
        exec(
            "/usr/bin/xcrun",
            &[
                "notarytool",
                "submit",
                text(&upload)?,
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
        let _ = fs::remove_file(upload);
    }

    let name = format!("Understudy-{version}-macos.zip");
    let zip = options.out.join(&name);
    let _ = fs::remove_file(&zip);
    exec(
        "/usr/bin/ditto",
        &["-c", "-k", "--keepParent", text(&app)?, text(&zip)?],
    )?;
    let bytes = fs::read(&zip).map_err(|error| error.to_string())?;
    let sums = format!("{:x}  {name}\n", Sha256::digest(&bytes));
    fs::write(options.out.join("SHA256SUMS"), &sums).map_err(|error| error.to_string())?;
    println!("\n{}\n{sums}", zip.display());
    if options.notarize.is_none() {
        println!("Not notarized: pass --notarize PROFILE for a release.");
    }
    Ok(())
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
    fn the_info_plist_is_valid_and_names_the_consented_executable() {
        let dir = std::env::temp_dir().join(format!("xtask-plist-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Info.plist");
        fs::write(&path, info_plist("1.2.3")).unwrap();
        let json = output(
            "/usr/bin/plutil",
            &["-convert", "json", "-o", "-", path.to_str().unwrap()],
        )
        .unwrap();
        fs::remove_dir_all(&dir).unwrap();
        for expected in [
            r#""CFBundleIdentifier":"com.insanearts.understudy""#,
            r#""CFBundleExecutable":"understudy""#,
            r#""CFBundleShortVersionString":"1.2.3""#,
            r#""LSUIElement":true"#,
        ] {
            assert!(json.contains(expected), "{expected} in {json}");
        }
    }
}
