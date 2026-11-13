use super::*;
use atlas_core::host::Host;

/// A home, and settings in it of the old form, naming the Second Login
/// alone, with the output folder `out`.
struct Station {
    home: tempfile::TempDir,
    config: PathBuf,
    output: PathBuf,
}

impl Station {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(home.path()).unwrap();
        let config = root.join("atlas.toml");
        let output = root.join("out");
        fs::write(&config, atlas_core::test_support::SETTINGS).unwrap();
        fs::DirBuilder::new().mode(0o700).create(&output).unwrap();
        Self {
            home,
            config,
            output,
        }
    }

    fn bindings(&self) -> Bindings {
        Bindings::in_home(self.home.path())
    }

    fn record(&self, session: &Key) {
        let sessions = self.output.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(sessions.join(format!("{}.json", session.as_str())), "{}").unwrap();
    }

    fn binding(&self, session: &Key) -> PathBuf {
        self.bindings().folder.join(session.as_str())
    }
}

fn session(name: &str) -> Key {
    Key::of(Host::Named, name)
}

fn elsewhere() -> RecordKey {
    RecordKey::parse("elsewhere").unwrap()
}

#[test]
fn a_command_with_no_session_takes_the_default_and_binds_nothing() -> Result<()> {
    let station = Station::new();
    let chosen = station
        .bindings()
        .choose(&station.config, &Selection::default(), None)?;
    assert_eq!(chosen.key, RecordKey::user());
    assert!(!chosen.told);
    assert!(!station.bindings().folder.exists());
    Ok(())
}

#[test]
fn a_new_session_is_bound_at_its_first_command() -> Result<()> {
    let station = Station::new();
    let one = session("one");
    let chosen = station
        .bindings()
        .choose(&station.config, &Selection::default(), Some(&one))?;
    assert_eq!(chosen.key, RecordKey::user());
    assert_eq!(fs::read_to_string(station.binding(&one))?, "user");
    Ok(())
}

#[test]
fn a_session_bound_to_a_desktop_no_longer_listed_is_refused_and_keeps_its_binding() -> Result<()> {
    let station = Station::new();
    let one = session("one");
    let bindings = station.bindings();
    bindings.keep(&station.binding(&one), elsewhere())?;
    let error = bindings
        .choose(&station.config, &Selection::default(), Some(&one))
        .unwrap_err();
    assert_eq!(error.code, Code::ConfigInvalid, "{error:?}");
    assert_eq!(fs::read_to_string(station.binding(&one))?, "elsewhere");
    Ok(())
}

#[test]
fn a_name_given_for_one_command_leaves_the_binding_as_it_is() -> Result<()> {
    let station = Station::new();
    let one = session("one");
    station
        .bindings()
        .keep(&station.binding(&one), elsewhere())?;
    let named = Selection {
        flag: Some("second-login".to_owned()),
        ..Selection::default()
    };
    let chosen = station
        .bindings()
        .choose(&station.config, &named, Some(&one))?;
    assert_eq!(chosen.key, RecordKey::user());
    assert_eq!(fs::read_to_string(station.binding(&one))?, "elsewhere");
    let unknown = Selection {
        flag: Some("elsewhere".to_owned()),
        ..Selection::default()
    };
    assert!(
        station
            .bindings()
            .choose(&station.config, &unknown, Some(&one))
            .is_err()
    );
    Ok(())
}

#[test]
fn an_unreadable_binding_refuses_even_with_a_name_given_and_is_kept() -> Result<()> {
    let station = Station::new();
    let one = session("one");
    let bindings = station.bindings();
    bindings.choose(&station.config, &Selection::default(), Some(&one))?;
    fs::write(station.binding(&one), "Not A Key")?;
    let named = Selection {
        flag: Some("second-login".to_owned()),
        ..Selection::default()
    };
    let error = bindings
        .choose(&station.config, &named, Some(&one))
        .unwrap_err();
    assert_eq!(error.code, Code::State);
    assert_eq!(fs::read_to_string(station.binding(&one))?, "Not A Key");
    Ok(())
}

#[test]
fn seeding_binds_every_recorded_session_to_the_second_login_once() -> Result<()> {
    let station = Station::new();
    let (recorded, bound) = (session("recorded"), session("bound"));
    station.record(&recorded);
    station.record(&bound);
    let bindings = station.bindings();
    bindings.keep(&station.binding(&bound), elsewhere())?;

    bindings.seed(&station.output)?;

    assert_eq!(fs::read_to_string(station.binding(&recorded))?, "user");
    assert_eq!(fs::read_to_string(station.binding(&bound))?, "elsewhere");
    assert!(station.output.join(SEEDED).exists());
    Ok(())
}

#[test]
fn a_session_with_a_record_from_before_bindings_is_the_second_logins() -> Result<()> {
    let station = Station::new();
    let one = session("one");
    station.record(&one);
    let chosen = station
        .bindings()
        .choose(&station.config, &Selection::default(), Some(&one))?;
    assert_eq!(chosen.key, RecordKey::user());
    assert!(!chosen.told);
    Ok(())
}

#[test]
fn bindings_made_at_once_with_different_desktops_leave_one_that_every_caller_sees() -> Result<()> {
    let station = Station::new();
    let path = station.binding(&session("one"));
    let start = std::sync::Barrier::new(8);
    let kept: Vec<RecordKey> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let (station, path, start) = (&station, &path, &start);
                scope.spawn(move || {
                    let candidate = RecordKey::parse(&format!("desktop-{index}")).unwrap();
                    start.wait();
                    station.bindings().keep(path, candidate).unwrap()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect()
    });
    let persisted = fs::read_to_string(&path)?;
    assert!(
        kept.iter().all(|key| key.as_str() == persisted),
        "{kept:?} {persisted}"
    );
    Ok(())
}

#[test]
fn choosing_waits_while_the_desktops_are_being_changed() -> Result<()> {
    let station = Station::new();
    let bindings = station.bindings();
    let changing = bindings.ordered(Lock::Exclusive)?;
    let (contended, waiting) = std::sync::mpsc::channel();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            atlas_core::files::on_next_lock_contention(contended);
            sender
                .send(bindings.choose(
                    &station.config,
                    &Selection::default(),
                    Some(&session("one")),
                ))
                .unwrap();
        });
        waiting.recv().unwrap();
        assert!(receiver.try_recv().is_err());
        drop(changing);
        assert!(receiver.recv().unwrap().is_ok());
    });
    Ok(())
}

#[test]
fn seeding_an_output_folder_no_command_used_yet_marks_it_seeded() -> Result<()> {
    let station = Station::new();
    let unused = station.output.join("not-yet");
    station.bindings().seed(&unused)?;
    assert!(unused.join(SEEDED).exists());
    Ok(())
}
