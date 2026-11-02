use super::*;

fn looks<'a>(
    original: &'a dyn Fn(&Path) -> Option<PathBuf>,
    bundle_id: &'a dyn Fn(&Path) -> Option<String>,
) -> Looks<'a> {
    Looks {
        processes: &|| Ok(Vec::new()),
        original,
        bundle_id,
    }
}

const TRANSLOCATED_PROBE: &str =
    "/private/var/folders/x/T/AppTranslocation/1F2E/d/Probe.app/Contents/MacOS/Probe";

#[test]
fn a_translocated_process_that_may_be_the_installs_keeps_it_and_is_never_proven() {
    let destination = Path::new("/Users/helper/Applications/Probe.app");
    let traced = |_: &Path| Some(destination.to_path_buf());
    let elsewhere = |_: &Path| Some(PathBuf::from("/Users/helper/Downloads/Probe.app"));
    let untraced = |_: &Path| None;
    let ours = |_: &Path| Some("test.atlas.install-probe".to_owned());
    let theirs = |_: &Path| Some("com.example.other".to_owned());
    let processes = vec![
        (1, format!("{}/Contents/MacOS/Probe", destination.display())),
        (2, TRANSLOCATED_PROBE.to_owned()),
        (3, "/Applications/Probe.app/Contents/MacOS/Probe".to_owned()),
        (
            4,
            format!("{}-old/Contents/MacOS/Probe", destination.display()),
        ),
    ];
    let id = Some("test.atlas.install-probe");
    let pids = |looks: &Looks| -> Vec<(u32, Runs)> {
        running_from(looks, &processes, destination, id)
            .into_iter()
            .map(|(pid, _, runs)| (pid, runs))
            .collect()
    };

    assert_eq!(
        pids(&looks(&traced, &theirs)),
        [(1, Runs::From), (2, Runs::Maybe)]
    );
    assert_eq!(
        pids(&looks(&elsewhere, &ours)),
        [(1, Runs::From), (2, Runs::Maybe)]
    );
    assert_eq!(pids(&looks(&elsewhere, &theirs)), [(1, Runs::From)]);
    assert_eq!(
        pids(&looks(&untraced, &ours)),
        [(1, Runs::From), (2, Runs::Maybe)]
    );
    assert_eq!(pids(&looks(&untraced, &theirs)), [(1, Runs::From)]);
    assert_eq!(
        translocated_bundle(Path::new(TRANSLOCATED_PROBE)),
        Some(PathBuf::from(
            "/private/var/folders/x/T/AppTranslocation/1F2E/d/Probe.app"
        ))
    );
}
