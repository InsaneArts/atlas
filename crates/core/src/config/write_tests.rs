use super::*;

const VM: &str = "0123456789abcdef01234567";
const OTHER_VM: &str = "89abcdef0123456789abcdef";

/// Old-form settings as a person writes them: comments throughout, keys
/// quoted, aligned and in an order of the person's own.
const OLD: &str = "# Atlas's settings\n\"input_socket\"   = \"i.sock\"\n\
                   output_dir       = \"out\" # runs\n\
                   # the helper's account\n\"helper_uid\"     = 60001 # the helper\n\
                   socket_path      = 'c.sock'\n\n\
                   # where the probe is\nidentity_probe   = \"probe\"\n\n\
                   # what pictures keep\n[activity]\nkeep_days = 3\n";

/// `OLD` as a list of its Second Login and the VM `work`: the file's own
/// comment stays at the top, and each field moves as written, with the
/// comments above it, in the person's order.
const CONVERTED: &str = "# Atlas's settings\noutput_dir       = \"out\" # runs\n\
                         default = \"second-login\"\n\n\
                         # what pictures keep\n[activity]\nkeep_days = 3\n\n\
                         [[desktops]]\nname = \"second-login\"\nkind = \"user\"\n\
                         \"input_socket\"   = \"i.sock\"\n\
                         # the helper's account\n\"helper_uid\"     = 60001 # the helper\n\
                         socket_path      = 'c.sock'\n\n\
                         # where the probe is\nidentity_probe   = \"probe\"\n\n\
                         [[desktops]]\nname = \"work\"\nkind = \"vm\"\n\
                         id = \"0123456789abcdef01234567\"\n";

fn id(text: &str) -> DesktopId {
    DesktopId::parse(text).unwrap()
}

fn add(name: &str, vm: &str) -> Change {
    Change::AddVm {
        name: name.to_owned(),
        id: id(vm),
    }
}

fn remove(vm: &str, disposition: Disposition) -> Change {
    Change::RemoveVm {
        id: id(vm),
        disposition,
        going: Vec::new(),
    }
}

fn settings_in(folder: &Path, text: &str) -> PathBuf {
    let path = folder.join("atlas.toml");
    std::fs::write(&path, text).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

fn names(path: &Path) -> Vec<String> {
    Settings::load(path)
        .unwrap()
        .desktops()
        .iter()
        .map(|listed| listed.name.clone())
        .collect()
}

#[test]
fn an_old_single_helper_file_becomes_a_list_byte_for_byte_and_is_kept_beside_it() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);

    assert_eq!(add("work", VM).apply(&path).unwrap(), Written::Changed);

    assert_eq!(std::fs::read_to_string(&path).unwrap(), CONVERTED);
    let kept = folder.path().join("atlas.toml.before-vm");
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), OLD);
    let settings = Settings::load(&path).unwrap();
    assert_eq!(settings.default_desktop().name, "second-login");
    assert_eq!(
        settings.default_desktop().record_key(),
        crate::desktop::RecordKey::user()
    );
}

#[test]
fn a_kept_old_file_that_holds_other_settings_is_never_overwritten_and_nothing_changes() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);
    let kept = folder.path().join("atlas.toml.before-vm");
    std::fs::write(&kept, "something else").unwrap();

    let refused = add("work", VM).apply(&path).unwrap_err();

    assert!(refused.error.contains("Move it aside"), "{}", refused.error);
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), "something else");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), OLD);
}

#[test]
fn with_no_settings_file_a_vm_is_listed_alone_as_the_default() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("config").join("atlas.toml");

    assert_eq!(add("work", VM).apply(&path).unwrap(), Written::Changed);

    let settings = Settings::load(&path).unwrap();
    assert_eq!(settings.desktops().len(), 1);
    assert_eq!(settings.default_desktop().name, "work");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert!(!folder.path().join("config/atlas.toml.before-vm").exists());
}

#[test]
fn a_list_keeps_every_other_line_and_gains_the_vm_at_its_end() {
    let folder = tempfile::tempdir().unwrap();
    let listed = "default = \"second-login\" # mine\n\n[[desktops]]\nname = \"second-login\"\n\
                  kind = \"user\"\nhelper_uid = 60001\nsocket_path = \"c.sock\"\n\
                  identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";
    let path = settings_in(folder.path(), listed);

    add("work", VM).apply(&path).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with(listed), "{text}");
    assert_eq!(names(&path), ["second-login", "work"]);
    assert!(!folder.path().join("atlas.toml.before-vm").exists());
}

#[test]
fn an_inline_list_gains_an_inline_entry() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(
        folder.path(),
        "default = \"a\"\ndesktops = [{ name = \"a\", kind = \"vm\", id = \"89abcdef0123456789abcdef\" }]\n",
    );

    add("work", VM).apply(&path).unwrap();

    assert_eq!(names(&path), ["a", "work"]);
}

#[test]
fn a_name_the_settings_already_list_is_refused_before_anything_is_written() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);

    let checked = add("second-login", VM).check(&path).unwrap_err();
    let applied = add("second-login", VM).apply(&path).unwrap_err();

    assert!(checked.error.contains("already list"), "{}", checked.error);
    assert_eq!(applied.error, checked.error);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), OLD);
    assert!(!folder.path().join("atlas.toml.before-vm").exists());
}

#[test]
fn a_check_says_what_the_settings_would_list_and_writes_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);

    let prospect = add("work", VM).check(&path).unwrap();

    let named: Vec<_> = prospect
        .desktops()
        .iter()
        .map(|listed| &listed.name)
        .collect();
    assert_eq!(named, ["second-login", "work"]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), OLD);
    let absent = folder.path().join("none.toml");
    assert_eq!(add("work", VM).check(&absent).unwrap().desktops().len(), 1);
    assert!(!absent.exists());
}

#[test]
fn a_default_vm_removed_hands_the_default_to_second_login() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);
    add("work", VM).apply(&path).unwrap();
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("default = \"second-login\"", "default = \"work\"");
    std::fs::write(&path, text).unwrap();

    remove(VM, Disposition::Desktop).apply(&path).unwrap();

    let settings = Settings::load(&path).unwrap();
    assert_eq!(names(&path), ["second-login"]);
    assert_eq!(settings.default_desktop().name, "second-login");
}

#[test]
fn a_default_vm_removed_without_second_login_hands_the_default_to_the_first_left() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("atlas.toml");
    add("work", VM).apply(&path).unwrap();
    add("play", OTHER_VM).apply(&path).unwrap();

    remove(VM, Disposition::Desktop).apply(&path).unwrap();

    assert_eq!(
        Settings::load(&path).unwrap().default_desktop().name,
        "play"
    );
}

#[test]
fn the_sole_desktop_is_refused_by_a_removal_and_left_as_it_is_by_uninstall() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("atlas.toml");
    add("work", VM).apply(&path).unwrap();
    let before = std::fs::read(&path).unwrap();

    let refused = remove(VM, Disposition::Desktop).check(&path).unwrap_err();
    let uninstalled = remove(VM, Disposition::Uninstall).apply(&path).unwrap();

    assert!(
        refused.error.contains("atlas uninstall"),
        "{}",
        refused.error
    );
    assert_eq!(uninstalled, Written::Unchanged);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn a_vm_no_longer_listed_is_already_removed() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);

    let removed = remove(VM, Disposition::Desktop).apply(&path).unwrap();

    assert_eq!(removed, Written::Unchanged);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), OLD);
}

#[test]
fn a_vm_is_found_by_its_id_whatever_it_is_named_now_and_another_of_its_old_name_is_left() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);
    add("work", VM).apply(&path).unwrap();
    let renamed = std::fs::read_to_string(&path)
        .unwrap()
        .replace("name = \"work\"", "name = \"job\"");
    std::fs::write(&path, renamed).unwrap();
    add("work", OTHER_VM).apply(&path).unwrap();

    let removed = remove(VM, Disposition::Desktop).apply(&path).unwrap();

    assert_eq!(removed, Written::Changed);
    assert_eq!(names(&path), ["second-login", "work"]);
    assert!(std::fs::read_to_string(&path).unwrap().contains(OTHER_VM));
    assert!(!std::fs::read_to_string(&path).unwrap().contains(VM));
}

#[test]
fn a_vm_being_removed_is_never_handed_the_default_nor_counted_as_left() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("atlas.toml");
    add("work", VM).apply(&path).unwrap();
    add("spare", OTHER_VM).apply(&path).unwrap();
    add("play", "fedcba9876543210fedcba98")
        .apply(&path)
        .unwrap();
    let going = |vms: &[&str]| Change::RemoveVm {
        id: id(VM),
        disposition: Disposition::Desktop,
        going: vms.iter().map(|vm| id(vm)).collect(),
    };

    let alone = going(&[OTHER_VM, "fedcba9876543210fedcba98"])
        .check(&path)
        .unwrap_err();
    let applied = going(&[OTHER_VM]).applied(&path).unwrap();

    assert!(alone.error.contains("not being removed"), "{}", alone.error);
    assert_eq!(applied.written, Written::Changed);
    assert_eq!(applied.default.as_deref(), Some("play"));
    assert_eq!(
        Settings::load(&path).unwrap().default_desktop().name,
        "play"
    );
}

#[test]
fn adds_racing_activity_changes_lose_none_of_them() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);
    let ids = [
        "000000000000000000000001",
        "000000000000000000000002",
        "000000000000000000000003",
        "000000000000000000000004",
    ];
    std::thread::scope(|scope| {
        for (at, vm) in ids.iter().enumerate() {
            let path = &path;
            scope.spawn(move || add(&format!("vm-{at}"), vm).apply(path).unwrap());
        }
        let path = &path;
        scope.spawn(move || super::super::keep_days(path, 5).unwrap());
        scope.spawn(move || super::super::typed_text_limit(path, Some(40)).unwrap());
    });

    let settings = Settings::load(&path).unwrap();
    assert_eq!(settings.desktops().len(), 1 + ids.len());
    let activity = settings.default_desktop().desktop.activity();
    assert_eq!(activity.keep_days, 5);
    assert_eq!(activity.typed_text_limit, Some(40));
}

#[test]
fn a_change_answering_nothing_leaves_the_file_and_makes_none() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("atlas.toml");

    assert_eq!(update(&path, |_| Ok(None)).unwrap(), Written::Unchanged);

    assert!(!path.exists());
}

#[test]
fn a_change_that_would_not_load_is_never_written() {
    let folder = tempfile::tempdir().unwrap();
    let path = settings_in(folder.path(), OLD);

    let refused = update(&path, |_| {
        Ok(Some(Edit::text("helper_uid = \"x\"\n".to_owned())))
    });

    assert!(refused.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), OLD);
}

#[test]
fn a_file_made_meanwhile_is_changed_rather_than_replaced() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("atlas.toml");
    let mut asked = Vec::new();

    update(&path, |current| {
        asked.push(current.map(str::to_owned));
        if current.is_none() {
            // Another writer makes the file before this one places its own.
            std::fs::write(&path, OLD).unwrap();
        }
        Ok(Some(Edit::text(
            OLD.replace("keep_days = 3", "keep_days = 4"),
        )))
    })
    .unwrap();

    assert_eq!(asked, [None, Some(OLD.to_owned())]);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("keep_days = 4")
    );
}
