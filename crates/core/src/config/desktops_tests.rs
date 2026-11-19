use super::*;
use crate::test_support::{CONFIGURED_UID, SETTINGS};

const LISTED: &str = "default = \"second-login\"\noutput_dir = \"out\"\n\n\
                      [[desktops]]\nname = \"second-login\"\nkind = \"user\"\n\
                      helper_uid = 60001\nsocket_path = \"c.sock\"\n\
                      identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";

fn parsed(text: &str) -> Result<Settings> {
    let directory = tempfile::tempdir()?;
    Settings::parse(text.as_bytes(), &directory.path().join("u.toml"))
}

fn refusal(text: &str) -> String {
    parsed(text).unwrap_err().error
}

#[test]
fn settings_that_name_one_helper_are_one_second_login_that_is_the_default() -> Result<()> {
    let settings = parsed(SETTINGS)?;
    assert_eq!(settings.desktops().len(), 1);
    assert_eq!(settings.default_desktop().name, SECOND_LOGIN);
    assert_eq!(
        settings
            .default_desktop()
            .desktop
            .user()
            .unwrap()
            .helper_uid,
        CONFIGURED_UID
    );
    Ok(())
}

#[test]
fn a_list_of_one_second_login_resolves_as_the_same_helper_as_the_old_form() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    let old = Settings::parse(SETTINGS.as_bytes(), &path)?;
    let listed = Settings::parse(LISTED.as_bytes(), &path)?;
    let (old, listed) = (
        old.resolve(&Selection::default())?,
        listed.resolve(&Selection::default())?,
    );
    assert_eq!(old, listed);
    assert_eq!(listed.record_key(), RecordKey::user());
    Ok(())
}

#[test]
fn settings_with_the_helper_both_at_the_top_and_in_a_list_are_refused() {
    let both = format!("helper_uid = 60001\n{LISTED}");
    assert!(refusal(&both).contains("belongs in its desktop's entry"));
}

#[test]
fn a_list_needs_a_default_it_contains() {
    let none = LISTED.replace("default = \"second-login\"\n", "");
    assert!(refusal(&none).contains("names no default"));
    let missing = LISTED.replace("default = \"second-login\"", "default = \"elsewhere\"");
    assert!(refusal(&missing).contains("is not in the list"));
}

#[test]
fn desktop_names_are_simple_words_and_unique() {
    for name in ["", "Second Login", "UPPER", "a/b", &"x".repeat(33)] {
        let text = LISTED
            .replace("name = \"second-login\"", &format!("name = \"{name}\""))
            .replace(
                "default = \"second-login\"",
                &format!("default = \"{name}\""),
            );
        assert!(refusal(&text).contains("is not a desktop name"), "{name}");
    }
    let entry = LISTED.split_once("[[desktops]]").unwrap().1;
    let twice = format!("{LISTED}\n[[desktops]]{entry}");
    assert!(refusal(&twice).contains("twice"));
}

#[test]
fn only_one_second_login_may_be_listed() {
    let entry = LISTED
        .split_once("[[desktops]]")
        .unwrap()
        .1
        .replace("name = \"second-login\"", "name = \"another\"");
    let two = format!("{LISTED}\n[[desktops]]{entry}");
    assert!(refusal(&two).contains("more than one Second Login"));
}

#[test]
fn an_unknown_kind_or_field_in_an_entry_is_refused() {
    assert!(parsed(&LISTED.replace("kind = \"user\"", "kind = \"cloud\"")).is_err());
    assert!(parsed(&LISTED.replace("kind = \"user\"\n", "")).is_err());
    assert!(parsed(&format!("{LISTED}colour = \"blue\"\n")).is_err());
}

#[test]
fn a_desktop_given_on_the_command_line_wins_over_a_binding() -> Result<()> {
    let settings = parsed(LISTED)?;
    let named = Selection {
        flag: Some(SECOND_LOGIN.to_owned()),
        bound: Some(RecordKey::parse("elsewhere").unwrap()),
        ..Selection::default()
    };
    assert_eq!(
        settings.resolve(&named)?.desktop.user().unwrap().helper_uid,
        CONFIGURED_UID
    );
    Ok(())
}

#[test]
fn a_desktop_the_settings_do_not_list_is_refused_never_replaced_by_the_default() -> Result<()> {
    let settings = parsed(LISTED)?;
    for selection in [
        Selection {
            flag: Some("elsewhere".to_owned()),
            ..Selection::default()
        },
        Selection {
            named: Some("elsewhere".to_owned()),
            ..Selection::default()
        },
        Selection {
            parent: Some("elsewhere".to_owned()),
            ..Selection::default()
        },
        Selection {
            parent: Some("user ".to_owned()),
            ..Selection::default()
        },
        Selection {
            bound: Some(RecordKey::parse("elsewhere").unwrap()),
            ..Selection::default()
        },
    ] {
        assert_eq!(
            settings.resolve(&selection).unwrap_err().code,
            Code::ConfigInvalid,
            "{selection:?}"
        );
    }
    Ok(())
}

#[test]
fn a_child_takes_the_desktop_its_parent_chose_over_the_one_its_environment_names() -> Result<()> {
    let settings = parsed(LISTED)?;
    let child = Selection {
        parent: Some("user".to_owned()),
        named: Some("elsewhere".to_owned()),
        ..Selection::default()
    };
    assert_eq!(settings.resolve(&child)?.record_key(), RecordKey::user());
    Ok(())
}

#[test]
fn a_repaired_second_login_keeps_its_record_key_but_not_its_id() -> Result<()> {
    let settings = parsed(LISTED)?;
    let repaired = parsed(&LISTED.replace("helper_uid = 60001", "helper_uid = 60002"))?;
    let (before, after) = (settings.user()?.desktop(), repaired.user()?.desktop());
    assert_eq!(before.record_key(), after.record_key());
    assert_ne!(before.id(), after.id());
    Ok(())
}

#[test]
fn a_repair_changes_only_the_second_logins_helper_however_the_list_is_written() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    let inline = "default = \"second-login\"\noutput_dir = \"out\"\ndesktops = [{ name = \
                  \"second-login\", kind = \"user\", helper_uid = 60001, socket_path = \
                  \"c.sock\", identity_probe = \"probe\", input_socket = \"i.sock\" }]\n";
    for text in [LISTED, inline] {
        let mut repaired = Settings::parse(text.as_bytes(), &path)?.user()?;
        repaired.helper_uid = 60002;
        let written = with_user(text, &path, &repaired)?;
        let settings = Settings::parse(written.as_bytes(), &path)?;
        assert_eq!(settings.user()?.helper_uid, 60002, "{written}");
        assert_eq!(settings.desktops().len(), 1);
    }
    Ok(())
}

#[test]
fn a_repair_keeps_the_comments_on_the_fields_it_changes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    let text = LISTED.replace("helper_uid = 60001", "helper_uid = 60001 # the helper");
    let mut repaired = Settings::parse(text.as_bytes(), &path)?.user()?;
    repaired.helper_uid = 60002;
    let written = with_user(&text, &path, &repaired)?;
    assert!(
        written.contains("helper_uid = 60002 # the helper"),
        "{written}"
    );
    Ok(())
}

#[test]
fn a_repair_that_would_outgrow_the_settings_file_is_refused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    let padding = format!("# {}\n", "x".repeat(16_000));
    let text = format!("{padding}{LISTED}");
    let mut repaired = Settings::parse(text.as_bytes(), &path)?.user()?;
    repaired.socket_path = directory.path().join("s".repeat(600));
    assert!(with_user(&text, &path, &repaired).is_err());
    Ok(())
}

const VM_ID: &str = "0123456789abcdef01234567";

/// `LISTED` with a VM desktop named `work` beside its Second Login.
fn with_vm() -> String {
    format!("{LISTED}\n[[desktops]]\nname = \"work\"\nkind = \"vm\"\nid = \"{VM_ID}\"\n")
}

/// Settings that list the VM desktop `work` alone.
const VM_ONLY: &str = "default = \"work\"\noutput_dir = \"out\"\n\n\
                       [[desktops]]\nname = \"work\"\nkind = \"vm\"\n\
                       id = \"0123456789abcdef01234567\"\n";

fn named(name: &str) -> Selection {
    Selection {
        flag: Some(name.to_owned()),
        ..Selection::default()
    }
}

#[test]
fn settings_with_a_vm_entry_load_and_resolve_it_by_name() -> Result<()> {
    let settings = parsed(&with_vm())?;
    assert_eq!(settings.desktops().len(), 2);
    let work = settings.resolve(&named("work"))?;
    let Chosen::Vm(vm) = &work.desktop else {
        panic!("work is a VM desktop: {work:?}");
    };
    assert_eq!(vm.id.as_str(), VM_ID);
    assert_eq!(
        work.record_key(),
        RecordKey::parse(&format!("vm-{VM_ID}")).unwrap()
    );
    assert_eq!(
        settings.resolve(&Selection::default())?.name,
        SECOND_LOGIN,
        "the default stays the Second Login"
    );
    let only = parsed(VM_ONLY)?;
    assert_eq!(only.default_desktop().name, "work");
    assert!(only.user().is_err(), "no Second Login is listed");
    Ok(())
}

#[test]
fn a_vm_identifier_that_is_not_24_lowercase_hex_digits_is_refused() {
    for id in [
        "",
        "0123",
        "0123456789ABCDEF01234567",
        "0123456789abcdef0123456g",
    ] {
        let text = with_vm().replace(VM_ID, id);
        assert!(refusal(&text).contains("is not a VM's identifier"), "{id}");
    }
}

#[test]
fn one_vm_listed_twice_is_refused_under_any_names() {
    let twice = format!(
        "{}\n[[desktops]]\nname = \"again\"\nkind = \"vm\"\nid = \"{VM_ID}\"\n",
        with_vm()
    );
    assert!(refusal(&twice).contains("twice"));
}

#[test]
fn a_second_login_beside_a_vm_keeps_its_identity_and_root_records() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    let alone = Settings::parse(LISTED.as_bytes(), &path)?;
    let beside = Settings::parse(with_vm().as_bytes(), &path)?;
    let (alone, beside) = (
        alone.resolve(&named(SECOND_LOGIN))?,
        beside.resolve(&named(SECOND_LOGIN))?,
    );
    assert_eq!(alone, beside);
    assert_eq!(
        beside.desktop.desktop().id().as_str(),
        "6a68ff188341e5b9ba04c83b",
        "the digest of helper:60001 that every earlier build gave it"
    );
    assert_eq!(beside.record_key(), RecordKey::user());
    assert_eq!(beside.desktop.records(), beside.desktop.output_dir());
    Ok(())
}

#[test]
fn a_vm_desktops_records_live_in_a_folder_of_its_own() -> Result<()> {
    let settings = parsed(&with_vm())?;
    let work = &settings.resolve(&named("work"))?.desktop;
    assert_eq!(
        work.records(),
        work.output_dir()
            .join("desktops")
            .join(format!("vm-{VM_ID}"))
    );
    assert_eq!(work.output_dir(), settings.user()?.output_dir);
    Ok(())
}

#[test]
fn repairing_the_second_login_keeps_the_vm_listed_beside_it() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    let text = with_vm();
    let mut repaired = Settings::parse(text.as_bytes(), &path)?.user()?;
    repaired.helper_uid = 60002;
    let written = with_user(&text, &path, &repaired)?;
    let settings = Settings::parse(written.as_bytes(), &path)?;
    assert_eq!(settings.user()?.helper_uid, 60002, "{written}");
    let work = settings.resolve(&named("work"))?;
    assert_eq!(work.record_key().as_str(), format!("vm-{VM_ID}"));
    Ok(())
}

#[test]
fn a_vm_desktop_is_refused_in_one_wording_that_names_the_second_login() -> Result<()> {
    let settings = parsed(&with_vm().replace("\"second-login\"", "\"main\""))?;
    let refused = settings
        .selected(settings.resolve(&named("work"))?)
        .unreachable();
    assert_eq!(refused.code, Code::Platform);
    assert_eq!(
        refused.error,
        "work is a VM desktop: this works only with Second Login."
    );
    assert_eq!(
        refused.details["next"],
        "Run it again with --desktop main to use Second Login."
    );
    Ok(())
}

#[test]
fn a_vm_desktop_alone_is_refused_with_no_other_desktop_to_try() -> Result<()> {
    let settings = parsed(VM_ONLY)?;
    let selected = settings.selected(settings.default_desktop());
    assert_eq!(selected.user_entry, None);
    let refused = selected.unreachable();
    assert_eq!(
        refused.error,
        "work is a VM desktop: this works only with Second Login."
    );
    assert!(!refused.error.contains("--desktop"));
    assert!(
        !refused.details["next"]
            .as_str()
            .unwrap()
            .contains("--desktop")
    );
    Ok(())
}

#[test]
fn a_vm_atlas_cannot_reach_now_says_why_and_how_to_see_it() -> Result<()> {
    let settings = parsed(&with_vm().replace("\"second-login\"", "\"main\""))?;
    let selected = settings.selected(settings.resolve(&named("work"))?);
    let refused = selected.not_reached(Some("work's session keeper is not running"));
    assert_eq!(refused.code, Code::Platform);
    assert_eq!(
        refused.error,
        "Atlas cannot reach work now: work's session keeper is not running."
    );
    assert_eq!(
        refused.details["next"],
        "Run atlas desktop list to see it, or run this again with --desktop main to use Second \
         Login."
    );
    assert_eq!(
        selected.not_reached(None).error,
        "Atlas cannot reach work now."
    );

    let alone = parsed(VM_ONLY)?;
    let refused = alone.selected(alone.default_desktop()).not_reached(None);
    assert_eq!(refused.details["next"], "Run atlas desktop list to see it.");
    Ok(())
}

#[test]
fn a_vm_desktop_does_not_load_as_a_second_logins_settings() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("u.toml");
    std::fs::write(&path, VM_ONLY)?;
    let selected = Selected::load(&path)?;
    assert_eq!(selected.name, "work");
    assert_eq!(
        Config::load(&path).unwrap_err().error,
        selected.unreachable().error
    );
    Ok(())
}

#[test]
fn a_vm_naming_the_same_desktop_as_the_second_login_is_refused_in_either_order() {
    let helper = crate::desktop::Desktop::from(crate::user::User {
        uid: CONFIGURED_UID,
    })
    .id();
    let vm = format!(
        "[[desktops]]\nname = \"work\"\nkind = \"vm\"\nid = \"{}\"\n",
        helper.as_str()
    );
    let user = "[[desktops]]\nname = \"second-login\"\nkind = \"user\"\n\
                helper_uid = 60001\nsocket_path = \"c.sock\"\n\
                identity_probe = \"probe\"\ninput_socket = \"i.sock\"\n";
    let head = "default = \"second-login\"\noutput_dir = \"out\"\n\n";

    for text in [format!("{head}{user}\n{vm}"), format!("{head}{vm}\n{user}")] {
        assert!(refusal(&text).contains("twice"), "{text}");
    }
}
