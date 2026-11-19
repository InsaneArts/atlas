use super::*;
use atlas_core::{
    config::desktops::Chosen,
    error::{Code, Error},
};
use std::path::Path;

/// The Second Login `main`, as settings list it.
const MAIN: &str = "[[desktops]]\nname = \"main\"\nkind = \"user\"\nhelper_uid = 60001\n\
                    socket_path = \"c.sock\"\nidentity_probe = \"probe\"\ninput_socket = \"i.sock\"\n\n";
/// The VM desktop `work`, as settings list it.
const WORK: &str =
    "[[desktops]]\nname = \"work\"\nkind = \"vm\"\nid = \"0123456789abcdef01234567\"\n\n";

/// Settings listing `listed` in its order, `main` the default.
fn settings(directory: &Path, listed: &[&str]) -> Settings {
    let text = format!(
        "default = \"main\"\noutput_dir = \"out\"\n\n{}",
        listed.concat()
    );
    Settings::parse(text.as_bytes(), &directory.join("u.toml")).unwrap()
}

/// The client of `listed`, its controller's state in `state`: Second Login's
/// input service is not there, nor the VM's keeper.
fn made(settings: &Settings, listed: &Listed, state: &Path) -> Result<Client> {
    let mut client = Client::new(settings.selected(listed))?;
    client.controller = atlas_control::Controller::of_at(&client.desktop(), state);
    Ok(client)
}

#[tokio::test]
async fn a_stop_keeps_a_vm_desktops_hold_and_says_it_did_not_reach_it_while_another_desktop_fails()
{
    let dir = tempfile::tempdir().unwrap();
    let settings = settings(dir.path(), &[MAIN, WORK]);
    let state = dir.path().join("state");
    let make = |listed: &Listed| match listed.desktop {
        Chosen::User(_) => Err(Error::new(Code::Io, "The helper could not be told")),
        Chosen::Vm(_) => made(&settings, listed, &state),
    };

    let error = stop_listed(&settings, None, make).await.unwrap_err();

    assert_eq!(error.error, "The helper could not be told");
    let each = &error.details["desktops"];
    assert_eq!(each[0]["desktop"], "main");
    assert_eq!(each[0]["uncertain"], true, "it failed");
    assert_eq!(each[1]["desktop"], "work");
    let work = &each[1]["reply"];
    assert_eq!(work["held"], true, "{work}");
    assert_eq!(work["delivered"], false, "{work}");
    assert_eq!(each[1]["uncertain"], true, "its guest was not told");
    let vm = made(&settings, &settings.desktops()[1], &state).unwrap();
    assert!(vm.controller.stop_noted(), "the VM's Stop is kept here");
}

#[tokio::test]
async fn a_stop_holds_every_desktop_in_either_order_and_names_the_uncertain_one() {
    for order in [[MAIN, WORK], [WORK, MAIN]] {
        let dir = tempfile::tempdir().unwrap();
        let settings = settings(dir.path(), &order);
        let state = dir.path().join("state");

        let stopped = stop_listed(&settings, None, |listed| made(&settings, listed, &state))
            .await
            .unwrap();

        let each = stopped["desktops"].as_array().unwrap();
        let names: Vec<_> = each.iter().map(|one| one["desktop"].clone()).collect();
        let listed: Vec<_> = settings
            .desktops()
            .iter()
            .map(|listed| json!(listed.name))
            .collect();
        assert_eq!(names, listed, "in the settings' order");
        for one in each {
            assert_eq!(one["reply"]["held"], true, "{one}");
            let vm = one["desktop"] == "work";
            assert_eq!(one["uncertain"], vm, "{one}");
        }
        assert_eq!(stopped["held"], true);
        assert_eq!(&stopped["delivered"], &each[0]["reply"]["delivered"]);
        for listed in settings.desktops() {
            let client = made(&settings, listed, &state).unwrap();
            assert!(client.controller.stop_noted(), "{} is held", listed.name);
        }
    }
}

#[tokio::test]
async fn with_one_desktop_its_own_stop_is_the_answer() {
    let dir = tempfile::tempdir().unwrap();
    let client = crate::test_support::client_in(dir.path());
    let alone = client.clone();

    let stopped = stop_each(vec![("main".to_owned(), Ok(client))])
        .await
        .unwrap();

    assert!(stopped.get("desktops").is_none(), "{stopped}");
    assert_eq!(stopped["held"], true);
    assert!(alone.controller.stop_noted());
}
