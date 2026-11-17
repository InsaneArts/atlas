use super::*;

fn atlas(path: &str) -> Provenance {
    Provenance {
        program: PathBuf::from(path),
        requirement: "identifier \"com.insanearts.atlas\" and anchor apple generic".to_owned(),
    }
}

#[test]
fn the_same_atlas_goes_on_and_one_elsewhere_stops_with_advice() {
    let made = atlas("/Applications/Atlas.app/Contents/MacOS/atlas");

    made.check(&made).unwrap();
    let moved = made
        .check(&atlas(
            "/Users/someone/Applications/Atlas.app/Contents/MacOS/atlas",
        ))
        .unwrap_err();
    let rebuilt = made
        .check(&Provenance {
            requirement: "cdhash H\"00\"".to_owned(),
            ..made.clone()
        })
        .unwrap_err();

    for refused in [moved, rebuilt] {
        assert_eq!(refused.code, Code::Identity);
        assert!(
            refused.error.contains("Run that Atlas again"),
            "{}",
            refused.error
        );
    }
}

#[test]
fn a_station_key_another_program_made_fails_the_add_before_a_vm_exists() {
    let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");
    let foreign = || {
        Err(Error::new(
            Code::Identity,
            "A Keychain item Atlas keeps was made by another program; Atlas does not use it.",
        ))
    };

    let refused = preflight_with(program, program, foreign).unwrap_err();

    assert_eq!(refused.code, Code::Identity);
    assert!(refused.error.contains("another path"), "{}", refused.error);
}

#[test]
fn an_atlas_run_from_outside_its_app_is_refused_before_the_key_is_read() {
    let read = std::cell::Cell::new(false);

    let refused = preflight_with(
        Path::new("/Users/someone/atlas/target/debug/atlas"),
        Path::new("/Applications/Atlas.app/Contents/MacOS/atlas"),
        || {
            read.set(true);
            Ok(())
        },
    )
    .unwrap_err();

    assert!(
        refused.error.contains("Run the atlas in Atlas.app"),
        "{}",
        refused.error
    );
    assert!(!read.get());
}

#[test]
fn a_station_key_this_program_reads_lets_the_add_go_on() {
    let program = Path::new("/Applications/Atlas.app/Contents/MacOS/atlas");

    preflight_with(program, program, || Ok(())).unwrap();
}
