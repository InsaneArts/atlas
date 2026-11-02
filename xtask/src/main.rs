//! Release tooling. `cargo xtask dist` builds Atlas.app for both Mac
//! architectures, signs it inside out with a Developer ID, checks it against
//! the same requirement setup enforces, optionally notarizes and staples it,
//! and writes a zip with its SHA-256 sum. The website repository's
//! scripts/publish-release.sh puts both on atlas.insanearts.io.
mod quality;

use atlas_cli::menubar::LOGIN_JOB;
use atlas_core::{
    app_bundle::{APP_EXECUTABLE, IDENTIFIER},
    files::exchange,
};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

/// The oldest macOS the app runs on, for every slice and the Info.plist.
const MINIMUM_MACOS: &str = "14.0";
const TARGETS: [(&str, &str); 2] = [
    ("aarch64-apple-darwin", "arm64"),
    ("x86_64-apple-darwin", "x86_64"),
];
/// Beside an app this tool published, so it never replaces anything else.
const MARKER: &str = ".atlas-dist";
/// What every property list opens and closes with.
const PLIST_HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
"#;
const PLIST_TAIL: &str = "</dict>\n</plist>\n";
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
        Some((command, rest)) if command == "quality" => quality::run(rest, &root()),
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
#[allow(
    clippy::expect_used,
    reason = "Cargo fixes this crate inside the repository"
)]
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

fn io<T>(result: std::io::Result<T>) -> Result<T> {
    result.map_err(|error| error.to_string())
}

fn info_plist(version: &str) -> String {
    format!(
        r#"{PLIST_HEAD}	<key>CFBundleIdentifier</key>
	<string>{IDENTIFIER}</string>
	<key>CFBundleName</key>
	<string>Atlas</string>
	<key>CFBundleDisplayName</key>
	<string>Atlas</string>
	<key>CFBundleExecutable</key>
	<string>atlas</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundleIconName</key>
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
{PLIST_TAIL}"#
    )
}

/// Compile the Icon Composer icon `icon` into the app's asset catalog, the
/// icon macOS 26 and later shape and light themselves. The partial plist
/// actool writes names only the icon keys Info.plist already carries.
fn compile_icon(icon: &Path, app: &Path, stage: &Path) -> Result<()> {
    let resources = app.join("Contents/Resources");
    exec(
        "/usr/bin/xcrun",
        &[
            "actool",
            text(icon)?,
            "--compile",
            text(&resources)?,
            "--platform",
            "macosx",
            "--minimum-deployment-target",
            MINIMUM_MACOS,
            "--app-icon",
            "AppIcon",
            "--output-partial-info-plist",
            text(&stage.join("AppIcon.plist"))?,
        ],
    )?;

    resources
        .join("Assets.car")
        .is_file()
        .then_some(())
        .ok_or_else(|| "actool wrote no Assets.car".to_owned())
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
        r#"{PLIST_HEAD}	<key>Label</key>
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
{PLIST_TAIL}"#
    )
}

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

/// Put the verified `staged` app at `out/Atlas.app`. The marker is
/// written first and the previous app is exchanged atomically, so an
/// interruption never leaves the output without an app this tool owns.
fn publish(staged: &Path, out: &Path) -> Result<PathBuf> {
    let app = out.join("Atlas.app");
    io(fs::write(out.join(MARKER), "Built by cargo xtask dist\n"))?;

    if app.exists() {
        io(exchange(staged, &app))?;
        // `staged` now holds the previous build; failing to remove it is harmless.
        let _ = fs::remove_dir_all(staged);
    } else {
        io(fs::rename(staged, &app))?;
    }

    Ok(app)
}

/// The version bump must be committed, Cargo.lock included: the builds are
/// --locked, and a tag must build the same archive again.
fn committed_version(root: &Path) -> Result<()> {
    let changed = output(
        "/usr/bin/git",
        &[
            "-C",
            text(root)?,
            "status",
            "--porcelain",
            "--",
            "Cargo.toml",
            "Cargo.lock",
        ],
    )?;
    if changed.trim().is_empty() {
        return Ok(());
    }

    Err("Cargo.toml or Cargo.lock has uncommitted changes; commit the version bump first".into())
}

/// A release's version must be plain numbers (`0.2.0`, not `0.2.0-beta`):
/// `atlas update` reads it from the archive's name and refuses any other.
fn release_version(version: &str) -> Result<()> {
    if atlas_client::update::exact_version(version).is_some() {
        return Ok(());
    }

    Err(format!(
        "Version {version} is not plain numbers such as 0.2.0, so atlas update would refuse it"
    ))
}

fn dist(options: &Options) -> Result<()> {
    let version = atlas_core::VERSION;
    release_version(version)?;
    committed_version(&root())?;
    publishable(&options.out)?;

    // A fresh staging directory per run; nothing is published until verified.
    let stage = options.out.join(format!("stage/{}", std::process::id()));
    if stage.exists() {
        io(fs::remove_dir_all(&stage))?;
    }

    io(fs::create_dir_all(&stage))?;
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

    // Without resource forks: unzip would otherwise leave ._ files inside the
    // bundle and break its seal.
    archive(&app, &zip)?;

    let sums = format!("{}  {name}\n", sha256(&zip)?);
    io(fs::write(&sums_path, &sums))?;
    println!("\n{}\n{sums}", zip.display());

    if options.notarize.is_none() {
        println!("Not notarized: pass --notarize PROFILE for a release.");
    }

    // Absolute, so the line pastes into the website repository as it is.
    let out = io(fs::canonicalize(&options.out))?;
    println!(
        "Publish it from the website repository: sh scripts/publish-release.sh {}",
        out.display()
    );

    Ok(())
}

fn archive(app: &Path, zip: &Path) -> Result<()> {
    exec(
        "/usr/bin/ditto",
        &[
            "-c",
            "-k",
            "--norsrc",
            "--keepParent",
            text(app)?,
            text(zip)?,
        ],
    )
}

fn sha256(path: &Path) -> Result<String> {
    let mut file = io(fs::File::open(path))?;
    let mut hasher = Sha256::new();
    io(std::io::copy(&mut file, &mut hasher))?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Build, sign and verify Atlas.app in `stage` (notarizing it through
/// `upload` when asked): the app, ready to publish.
fn build(options: &Options, stage: &Path, upload: &Path) -> Result<PathBuf> {
    let root = root();
    let app = compile_executables(&root, stage)?;
    assemble_resources(&root, &app, stage)?;
    sign_and_verify(options, &app, upload)?;
    Ok(app)
}

fn compile_executables(root: &Path, stage: &Path) -> Result<PathBuf> {
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
                "atlas-cli",
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

    io(fs::create_dir_all(&macos))?;
    io(fs::create_dir_all(app.join("Contents/Resources")))?;
    io(fs::write(
        app.join("Contents/Info.plist"),
        info_plist(atlas_core::VERSION),
    ))?;

    universal(&cli, &macos.join("atlas"))?;
    io(fs::copy(&executable, macos.join(APP_EXECUTABLE)))?;
    check_universal(&macos.join(APP_EXECUTABLE))?;
    Ok(app)
}

/// Package the app's maintained resources and matching committed agent plugin.
fn assemble_resources(root: &Path, app: &Path, stage: &Path) -> Result<()> {
    let package = root.join("app");
    compile_icon(&package.join("Resources/AppIcon.icon"), app, stage)?;

    // The app icon for macOS before 26 and the helper account's picture,
    // both drawn by app/Resources/AppIcon.swift, plus original agent logos
    // extracted from the installed apps. The icon replaces actool's flat one.
    for file in [
        "AppIcon.icns",
        "HelperPicture.png",
        "ClaudeLogo.png",
        "ClaudeLogo@2x.png",
        "CodexLogo.png",
        "CodexLogo@2x.png",
        "CursorLogo.png",
        "CursorLogo@2x.png",
    ] {
        io(fs::copy(
            package.join("Resources").join(file),
            app.join("Contents/Resources").join(file),
        ))?;
    }

    // The agent plugin (Claude Code and Codex), matching this CLI's version:
    // its committed files only, so nothing a test or tool left beside them
    // ships, and not its tests.
    let tracked = output(
        "/usr/bin/git",
        &["-C", text(root)?, "ls-files", "--", "agents"],
    )?;
    for file in tracked
        .lines()
        .filter(|file| !file.split('/').any(|part| part == "tests"))
    {
        let target = app.join("Contents/Resources").join(file);
        if let Some(folder) = target.parent() {
            io(fs::create_dir_all(folder))?;
        }
        // ditto keeps the mode, so the plugin's scripts stay executable.
        exec("/usr/bin/ditto", &[text(&root.join(file))?, text(&target)?])?;
    }

    // "Open at login": the menu bar registers this agent with SMAppService.
    let agents = app.join("Contents/Library/LaunchAgents");
    io(fs::create_dir_all(&agents))?;
    io(fs::write(
        agents.join(format!("{LOGIN_JOB}.plist")),
        menubar_agent(),
    ))?;

    Ok(())
}

/// A distributed app must be readable across logins, independently of the build
/// umask and cached compiler output modes. Only this fresh staged bundle changes.
fn bundle_permissions(app: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(app)
        .map_err(|error| format!("Inspect bundle entry {}: {error}", app.display()))?;
    let mode = if metadata.is_dir() {
        0o755
    } else if metadata.is_file() {
        if metadata.permissions().mode() & 0o111 != 0 {
            0o755
        } else {
            0o644
        }
    } else {
        return Err(format!(
            "Bundle entry {} is not a plain file or directory",
            app.display()
        ));
    };
    fs::set_permissions(app, fs::Permissions::from_mode(mode))
        .map_err(|error| format!("Set bundle permissions on {}: {error}", app.display()))?;
    if metadata.is_dir() {
        let entries = fs::read_dir(app)
            .map_err(|error| format!("Read bundle directory {}: {error}", app.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("Read bundle entry in {}: {error}", app.display()))?;
            bundle_permissions(&entry.path())?;
        }
    }
    Ok(())
}

fn sign_and_verify(options: &Options, app: &Path, upload: &Path) -> Result<()> {
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
        Some(atlas_core::app_bundle::APP_IDENTIFIER),
        &app.join("Contents/MacOS").join(APP_EXECUTABLE),
    )?;
    sign(None, app)?;
    // Signing may create a private _CodeSignature directory under a restrictive
    // umask. Normalize it too, then verify the unchanged signed contents.
    bundle_permissions(app)?;

    exec(
        "/usr/bin/codesign",
        &["--verify", "--strict", "--deep", text(app)?],
    )?;

    if !atlas_core::app_bundle::developer_id(&app.join("Contents/MacOS/atlas")) {
        return Err(
            "the app does not satisfy setup's Developer ID requirement (identity and team)".into(),
        );
    }

    if let Some(profile) = &options.notarize {
        archive(app, upload)?;

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

        exec("/usr/bin/xcrun", &["stapler", "staple", text(app)?])?;
        exec(
            "/usr/sbin/spctl",
            &["--assess", "--type", "execute", "-vv", text(app)?],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests;
