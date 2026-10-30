use super::*;
use crate::agents::{Change, Covering};
use std::{
    process::Stdio,
    time::{Duration, Instant},
};

const HOST: &str = r#"#!/bin/sh
set -eu
host=${0##*/}
state="$HOME/$host"
cache="$HOME/.$host/plugins/cache/atlas/atlas/0.1.0"
if [ -f "$state/local" ]; then cache="$state/local-plugin"; fi
printf '%s\n' "$*" >> "$state/calls"
case "$*" in
  --version) printf 'test host\n'; exit 0 ;;
  'plugin marketplace list --json')
    if [ -f "$state/registered" ]; then
      exec /bin/cat "$state/source.json"
    elif [ "$host" = claude ]; then
      printf '[]'
    else
      printf '{"marketplaces":[]}'
    fi
    exit 0 ;;
  'plugin list --json'|'plugin list --marketplace atlas --json')
    if [ -d "$cache" ] || [ -f "$state/scoped" ]; then
      if [ "$host" = claude ]; then
        enabled=true
        if [ -f "$HOME/.claude/settings.json" ]; then
          IFS= read -r settings < "$HOME/.claude/settings.json" || true
          case "$settings" in *'"atlas@atlas":false'*) enabled=false ;; esac
        fi
        if [ -f "$state/project-settings.json" ]; then
          IFS= read -r settings < "$state/project-settings.json" || true
          case "$settings" in
            *'"atlas@atlas":true'*) enabled=true ;;
            *'"atlas@atlas":false'*) enabled=false ;;
          esac
        fi
        exec /usr/bin/sed "s/\"enabled\":true/\"enabled\":$enabled/g" "$state/installed.json"
      fi
      exec /bin/cat "$state/installed.json"
    elif [ "$host" = claude ]; then
      printf '[]'
    else
      if [ ! -f "$state/registered" ]; then exit 1; fi
      printf '{"installed":[]}'
    fi
    exit 0 ;;
esac
printf '%s\n' "$*" >> "$state/changes"
if [ -f "$state/noop" ]; then exit 0; fi
case "$*" in
  'plugin marketplace add '*) : > "$state/registered" ;;
  'plugin marketplace remove atlas')
    /bin/rm -rf "$state/registered" "$state/project" "$state/local-registration" "$cache" ;;
  'plugin uninstall atlas@atlas --scope user'|'plugin remove atlas@atlas')
    /bin/rm -rf "$cache" ;;
  'plugin enable atlas@atlas --scope user')
    if [ -f "$HOME/.claude/settings.json" ]; then
      IFS= read -r settings < "$HOME/.claude/settings.json" || true
      case "$settings" in *'"atlas@atlas":true'*) exit 1 ;; esac
    fi
    printf '{"enabledPlugins":{"atlas@atlas":true}}\n' > "$HOME/.claude/settings.json" ;;
  'plugin disable atlas@atlas --scope user')
    if [ ! -f "$state/ignore-disable" ]; then
      printf '{"enabledPlugins":{"atlas@atlas":false}}\n' > "$HOME/.claude/settings.json"
    fi ;;
  'plugin install atlas@atlas --scope user'|'plugin add atlas@atlas')
    if [ "$host" = claude ] && [ -d "$cache" ]; then exit 0; fi
    /bin/rm -rf "$cache"
    /bin/mkdir -p "${cache%/*}"
    if [ "$host" = codex ]; then
      printf '[plugins."atlas@atlas"]\nenabled = true\n' > "$HOME/.codex/config.toml"
    else
      /bin/mkdir -p "$HOME/.claude"
      printf '{"enabledPlugins":{"atlas@atlas":true}}\n' > "$HOME/.claude/settings.json"
    fi
    if [ -f "$state/user-installed.json" ]; then
      /bin/cp "$state/user-installed.json" "$state/installed.json"
    fi
    IFS= read -r plugin < "$state/plugin-path" || true
    exec /bin/cp -R "$plugin" "$cache" ;;
  *) exit 67 ;;
esac
"#;

fn isolated(test: &str) -> bool {
    const CHILD: &str = "ATLAS_CACHED_PLUGIN_TEST";
    if std::env::var_os(CHILD).is_some() {
        return true;
    }
    let directory = tempfile::tempdir().unwrap();
    let bin = directory.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for host in ["claude", "codex", "opencode", "pi", "cursor-agent"] {
        let path = bin.join(host);
        fs::write(
            &path,
            if matches!(host, "claude" | "codex") {
                HOST
            } else {
                "#!/bin/sh\nexit 1\n"
            },
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut child = crate::test_support::rerun(test)
        .env_clear()
        .env(CHILD, "1")
        .env("HOME", directory.path())
        .env("CLAUDE_CONFIG_DIR", directory.path().join(".claude"))
        .env("CODEX_HOME", directory.path().join(".codex"))
        .env("TMPDIR", directory.path())
        .env("PATH", bin)
        .env("SHELL", "/usr/bin/false")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            let calls: Vec<_> = ["claude", "codex"]
                .map(|host| {
                    let calls = fs::read_to_string(directory.path().join(host).join("calls"))
                        .unwrap_or_default();
                    format!("{host}:\n{calls}")
                })
                .into();
            panic!(
                "the fake host test exceeded 15 seconds\n{}",
                calls.join("\n")
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
    false
}

struct Fixture {
    agent: Agent,
    state: PathBuf,
    cache: PathBuf,
}

impl Fixture {
    fn new(agent: Agent) -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let state = home.join(agent.name());
        fs::create_dir_all(&state).unwrap();
        let marketplace = crate::agents::marketplace().unwrap();
        let cache = agent
            .home()
            .unwrap()
            .join("plugins/cache/atlas/atlas/0.1.0");
        let source = match agent {
            Agent::Claude => serde_json::json!([{
                "name": "atlas", "source": "directory", "path": marketplace
            }]),
            _ => serde_json::json!({"marketplaces": [{
                "name": "atlas", "marketplaceSource": {
                    "sourceType": "local", "source": marketplace
                }
            }]}),
        };
        let installed = match agent {
            Agent::Claude => serde_json::json!([{
                "id": "atlas@atlas", "scope": "user", "version": "0.1.0",
                "installPath": cache, "enabled": true
            }]),
            _ => serde_json::json!({"installed": [{
                "pluginId": "atlas@atlas", "version": "0.1.0"
            }]}),
        };
        fs::write(state.join("source.json"), source.to_string()).unwrap();
        fs::write(state.join("installed.json"), installed.to_string()).unwrap();
        fs::write(
            state.join("plugin-path"),
            marketplace.join("atlas").to_str().unwrap(),
        )
        .unwrap();
        Self {
            agent,
            state,
            cache,
        }
    }

    fn install(&self) -> Value {
        crate::agents::change_these(&[self.agent], Covering::default(), Change::Connect).unwrap()
    }

    fn stale(&self) {
        fs::write(self.cache.join("skills/atlas/SKILL.md"), "old skill").unwrap();
    }

    fn current(&self) {
        assert_eq!(
            payload(&self.cache).unwrap(),
            payload(&crate::agents::marketplace().unwrap().join("atlas")).unwrap()
        );
    }
}

#[test]
fn installation_refreshes_the_same_version_and_repeats_without_changes() {
    if !isolated(
        "agents::cached::tests::installation_refreshes_the_same_version_and_repeats_without_changes",
    ) {
        return;
    }
    for agent in [Agent::Claude, Agent::Codex] {
        let fixture = Fixture::new(agent);
        let first = fixture.install();
        assert_eq!(first["agents"][0]["changed"], true, "{first}");
        fixture.current();
        fixture.stale();
        let refreshed = fixture.install();
        assert_eq!(refreshed["agents"][0]["changed"], true, "{refreshed}");
        fixture.current();
        if agent == Agent::Claude {
            let changes = fs::read_to_string(fixture.state.join("changes")).unwrap();
            assert!(!changes.contains("plugin enable"), "{changes}");
            assert!(
                run(
                    agent,
                    &argv(&["plugin", "enable", super::super::PLUGIN, "--scope", "user"]),
                )
                .is_err()
            );
        }
        let changes = fs::read(fixture.state.join("changes")).unwrap();
        let repeated = fixture.install();
        assert_eq!(repeated["agents"][0]["changed"], false, "{repeated}");
        assert_eq!(fs::read(fixture.state.join("changes")).unwrap(), changes);
    }
}

#[test]
fn installation_restores_missing_files_and_removes_obsolete_files() {
    if !isolated(
        "agents::cached::tests::installation_restores_missing_files_and_removes_obsolete_files",
    ) {
        return;
    }
    for agent in [Agent::Claude, Agent::Codex] {
        let fixture = Fixture::new(agent);
        assert_eq!(fixture.install()["ok"], true);
        fs::write(fixture.cache.join("obsolete.md"), "removed reference").unwrap();
        fs::remove_file(fixture.cache.join("skills/setup/SKILL.md")).unwrap();
        assert_eq!(fixture.install()["agents"][0]["changed"], true);
        fixture.current();
    }
}

#[test]
fn installation_reconnects_a_disabled_codex_plugin() {
    if !isolated("agents::cached::tests::installation_reconnects_a_disabled_codex_plugin") {
        return;
    }
    let fixture = Fixture::new(Agent::Codex);
    assert_eq!(fixture.install()["ok"], true);
    fs::write(
        Agent::Codex.home().unwrap().join("config.toml"),
        "[plugins.\"atlas@atlas\"]\nenabled = false\n",
    )
    .unwrap();
    fixture.current();
    let changes = fs::read(fixture.state.join("changes")).unwrap();
    assert_eq!(crate::agents::refresh()["agents"][0]["ok"], true);
    assert_eq!(fs::read(fixture.state.join("changes")).unwrap(), changes);
    assert!(!enabled(Agent::Codex, installed(Agent::Codex).unwrap().as_ref()).unwrap());

    fs::write(fixture.state.join("noop"), "").unwrap();
    let failed = fixture.install();
    assert_eq!(failed["ok"], false, "{failed}");
    assert!(failed["agents"][0]["changed"].is_null());
    fs::remove_file(fixture.state.join("noop")).unwrap();

    let reconnected = fixture.install();
    assert_eq!(reconnected["ok"], true, "{reconnected}");
    assert_eq!(reconnected["agents"][0]["changed"], true, "{reconnected}");
    assert!(enabled(Agent::Codex, installed(Agent::Codex).unwrap().as_ref()).unwrap());
    fixture.current();
    let changes = fs::read(fixture.state.join("changes")).unwrap();
    assert_eq!(fixture.install()["agents"][0]["changed"], false);
    assert_eq!(fs::read(fixture.state.join("changes")).unwrap(), changes);
}

#[test]
fn installation_enables_a_current_claude_plugin_at_user_scope() {
    if !isolated(
        "agents::cached::tests::installation_enables_a_current_claude_plugin_at_user_scope",
    ) {
        return;
    }
    let fixture = Fixture::new(Agent::Claude);
    assert_eq!(fixture.install()["ok"], true);
    fs::write(
        Agent::Claude.home().unwrap().join("settings.json"),
        "{\"enabledPlugins\":{\"atlas@atlas\":false}}\n",
    )
    .unwrap();
    assert_eq!(
        installed(Agent::Claude).unwrap().unwrap().enabled,
        Some(false)
    );
    fixture.current();
    fs::write(fixture.state.join("changes"), "").unwrap();

    fs::write(fixture.state.join("noop"), "").unwrap();
    let failed = fixture.install();
    assert_eq!(failed["ok"], false, "{failed}");
    assert!(failed["agents"][0]["changed"].is_null());
    assert_eq!(user_enablement().unwrap(), Some(false));
    assert_eq!(
        fs::read_to_string(fixture.state.join("changes")).unwrap(),
        "plugin enable atlas@atlas --scope user\n"
    );
    fs::write(
        fixture.state.join("project-settings.json"),
        "{\"enabledPlugins\":{\"atlas@atlas\":true}}\n",
    )
    .unwrap();
    assert_eq!(
        installed(Agent::Claude).unwrap().unwrap().enabled,
        Some(true)
    );
    assert_eq!(fixture.install()["ok"], false);
    fs::remove_file(fixture.state.join("noop")).unwrap();

    let enabled = fixture.install();
    assert_eq!(enabled["ok"], true, "{enabled}");
    assert_eq!(enabled["agents"][0]["changed"], true, "{enabled}");
    assert_eq!(user_enablement().unwrap(), Some(true));
    fixture.current();
    let changes = fs::read(fixture.state.join("changes")).unwrap();
    assert_eq!(fixture.install()["agents"][0]["changed"], false);
    assert_eq!(fs::read(fixture.state.join("changes")).unwrap(), changes);
}

#[test]
fn claude_refresh_keeps_a_disabled_plugin_disabled() {
    if !isolated("agents::cached::tests::claude_refresh_keeps_a_disabled_plugin_disabled") {
        return;
    }
    let fixture = Fixture::new(Agent::Claude);
    assert_eq!(fixture.install()["ok"], true);
    let settings = Agent::Claude.home().unwrap().join("settings.json");
    let disabled = "{\"enabledPlugins\":{\"atlas@atlas\":false}}\n";
    fs::write(&settings, disabled).unwrap();
    let project = fixture.state.join("project-settings.json");
    let project_settings = "{\"enabledPlugins\":{\"atlas@atlas\":true}}\n";
    fs::write(&project, project_settings).unwrap();
    assert_eq!(
        installed(Agent::Claude).unwrap().unwrap().enabled,
        Some(true)
    );
    fixture.stale();
    let refreshed = crate::agents::refresh();
    assert_eq!(refreshed["agents"][0]["ok"], true, "{refreshed}");
    fixture.current();
    assert_eq!(fs::read_to_string(&settings).unwrap(), disabled);
    assert_eq!(fs::read_to_string(&project).unwrap(), project_settings);
    let changes = fs::read_to_string(fixture.state.join("changes")).unwrap();
    assert!(changes.contains("plugin disable atlas@atlas --scope user"));
    let calls = fs::read_to_string(fixture.state.join("calls")).unwrap();
    assert!(calls.contains(
        "plugin install atlas@atlas --scope user\nplugin disable atlas@atlas --scope user\n"
    ));
    assert_eq!(crate::agents::refresh()["agents"][0]["ok"], true);
    assert_eq!(
        fs::read_to_string(fixture.state.join("changes")).unwrap(),
        changes
    );
    assert_eq!(fs::read_to_string(&settings).unwrap(), disabled);

    fixture.stale();
    fs::write(fixture.state.join("ignore-disable"), "").unwrap();
    let failed = crate::agents::refresh();
    assert_eq!(failed["agents"][0]["ok"], false, "{failed}");
}

#[test]
fn claude_refresh_keeps_enabled_and_absent_user_settings() {
    if !isolated("agents::cached::tests::claude_refresh_keeps_enabled_and_absent_user_settings") {
        return;
    }
    let fixture = Fixture::new(Agent::Claude);
    assert_eq!(fixture.install()["ok"], true);
    let settings = Agent::Claude.home().unwrap().join("settings.json");
    fs::write(
        fixture.state.join("project-settings.json"),
        "{\"enabledPlugins\":{\"atlas@atlas\":false}}\n",
    )
    .unwrap();
    for value in [Some(true), None] {
        let plugins = value.map_or_else(
            || serde_json::json!({}),
            |value| serde_json::json!({"atlas@atlas": value}),
        );
        fs::write(
            &settings,
            serde_json::json!({"enabledPlugins": plugins}).to_string(),
        )
        .unwrap();
        assert_eq!(
            installed(Agent::Claude).unwrap().unwrap().enabled,
            Some(false)
        );
        fixture.stale();
        let refreshed = crate::agents::refresh();
        assert_eq!(refreshed["agents"][0]["ok"], true, "{refreshed}");
        fixture.current();
        assert_eq!(user_enablement().unwrap(), value);
    }
    let changes = fs::read_to_string(fixture.state.join("changes")).unwrap();
    assert!(!changes.contains("plugin enable"), "{changes}");
    fs::remove_file(&settings).unwrap();
    fixture.stale();
    let refreshed = crate::agents::refresh();
    assert_eq!(refreshed["agents"][0]["ok"], true, "{refreshed}");
    fixture.current();
    assert_eq!(user_enablement().unwrap(), None);
}

#[test]
fn codex_installation_checks_effective_enablement_before_and_after_installing() {
    if !isolated(
        "agents::cached::tests::codex_installation_checks_effective_enablement_before_and_after_installing",
    ) {
        return;
    }
    let fixture = Fixture::new(Agent::Codex);
    assert_eq!(fixture.install()["ok"], true);
    let config = Agent::Codex.home().unwrap().join("config.toml");
    let installed = fixture.state.join("installed.json");
    let mut entry: Value = serde_json::from_slice(&fs::read(&installed).unwrap()).unwrap();
    entry["installed"][0]["enabled"] = serde_json::json!(true);
    fs::write(&installed, entry.to_string()).unwrap();
    fs::write(&config, "[plugins.\"atlas@atlas\"]\nenabled = false\n").unwrap();
    let changes = fs::read(fixture.state.join("changes")).unwrap();
    let unchanged = fixture.install();
    assert_eq!(unchanged["ok"], true, "{unchanged}");
    assert_eq!(unchanged["agents"][0]["changed"], false, "{unchanged}");
    assert_eq!(fs::read(fixture.state.join("changes")).unwrap(), changes);

    entry["installed"][0]["enabled"] = serde_json::json!(false);
    fs::write(&installed, entry.to_string()).unwrap();
    let user_enabled = "[plugins.\"atlas@atlas\"]\nenabled = true\n";
    fs::write(&config, user_enabled).unwrap();
    let failed = fixture.install();
    assert_eq!(failed["ok"], false, "{failed}");
    assert!(failed["agents"][0]["changed"].is_null());
    assert!(
        failed["agents"][0]["error"]
            .as_str()
            .unwrap()
            .contains("disabled")
    );
    assert_ne!(fs::read(fixture.state.join("changes")).unwrap(), changes);
    assert_eq!(fs::read_to_string(config).unwrap(), user_enabled);
    assert_eq!(fs::read_to_string(installed).unwrap(), entry.to_string());
    fixture.current();
}

fn claude_refresh_without_a_user_installation(scope: &str) {
    let mut fixture = Fixture::new(Agent::Claude);
    assert_eq!(fixture.install()["ok"], true);
    let user_cache = fixture.cache.clone();
    fixture.cache = fixture.state.join("scoped-plugin");
    fs::rename(&user_cache, &fixture.cache).unwrap();
    fs::write(fixture.state.join("scoped"), "").unwrap();
    let user = fs::read(fixture.state.join("installed.json")).unwrap();
    let mut scoped: Value = serde_json::from_slice(&user).unwrap();
    scoped[0]["scope"] = serde_json::json!(scope);
    scoped[0]["installPath"] = serde_json::json!(fixture.cache);
    let mut both = scoped.clone();
    both.as_array_mut()
        .unwrap()
        .push(serde_json::from_slice::<Value>(&user).unwrap()[0].clone());
    let scoped = scoped.to_string();
    fs::write(fixture.state.join("installed.json"), &scoped).unwrap();
    fs::remove_file(fixture.state.join("changes")).unwrap();
    for stale in [false, true] {
        if stale {
            fixture.stale();
        }
        let before = payload(&fixture.cache).unwrap();
        let refreshed = crate::agents::refresh();
        assert_eq!(refreshed["agents"][0]["ok"], true, "{refreshed}");
        assert!(!fixture.state.join("changes").exists());
        assert!(!user_cache.exists());
        assert_eq!(payload(&fixture.cache).unwrap(), before);
        assert_eq!(
            fs::read_to_string(fixture.state.join("installed.json")).unwrap(),
            scoped
        );
    }
    let before = payload(&fixture.cache).unwrap();
    fs::write(fixture.state.join("user-installed.json"), both.to_string()).unwrap();
    let installed = fixture.install();
    assert_eq!(installed["ok"], true, "{installed}");
    assert_eq!(installed["agents"][0]["changed"], true, "{installed}");
    assert_eq!(payload(&fixture.cache).unwrap(), before);
    fixture.cache = user_cache;
    fixture.current();
    let changes = fs::read_to_string(fixture.state.join("changes")).unwrap();
    assert!(changes.contains("plugin install atlas@atlas --scope user"));
    assert!(!changes.contains("uninstall"), "{changes}");
}

#[test]
fn claude_refresh_skips_a_project_only_installation() {
    if isolated("agents::cached::tests::claude_refresh_skips_a_project_only_installation") {
        claude_refresh_without_a_user_installation("project");
    }
}

#[test]
fn claude_refresh_skips_a_local_only_installation() {
    if isolated("agents::cached::tests::claude_refresh_skips_a_local_only_installation") {
        claude_refresh_without_a_user_installation("local");
    }
}

#[test]
fn claude_refresh_preserves_project_and_local_registrations() {
    if !isolated("agents::cached::tests::claude_refresh_preserves_project_and_local_registrations")
    {
        return;
    }
    let fixture = Fixture::new(Agent::Claude);
    assert_eq!(fixture.install()["ok"], true);
    let mut installed: Value =
        serde_json::from_slice(&fs::read(fixture.state.join("installed.json")).unwrap()).unwrap();
    let mut preserved = Vec::new();
    for scope in ["project", "local-registration"] {
        let registration = fixture.state.join(scope);
        fs::create_dir(&registration).unwrap();
        let plugin = serde_json::json!({
            "id": "atlas@atlas", "scope": if scope == "project" { "project" } else { "local" },
            "installPath": registration.join("plugin")
        });
        installed.as_array_mut().unwrap().insert(0, plugin.clone());
        for (name, value) in [
            (
                "marketplace.json",
                serde_json::json!({"atlas": {"source": "directory", "path": registration}}),
            ),
            ("installed.json", plugin),
        ] {
            let path = registration.join(name);
            let bytes = value.to_string().into_bytes();
            fs::write(&path, &bytes).unwrap();
            preserved.push((path, bytes));
        }
    }
    fs::write(fixture.state.join("installed.json"), installed.to_string()).unwrap();
    fixture.stale();
    assert_eq!(fixture.install()["agents"][0]["changed"], true);
    fixture.current();
    fixture.stale();
    let refreshed = crate::agents::refresh();
    assert_eq!(refreshed["agents"][0]["ok"], true, "{refreshed}");
    fixture.current();
    for (path, bytes) in preserved {
        assert_eq!(fs::read(&path).unwrap(), bytes, "{}", path.display());
    }
    let changes = fs::read_to_string(fixture.state.join("changes")).unwrap();
    assert!(!changes.contains("marketplace remove"), "{changes}");
    assert_eq!(
        changes
            .matches("plugin uninstall atlas@atlas --scope user")
            .count(),
        2
    );
}

#[test]
fn claude_installs_and_refreshes_plugins_loaded_from_a_folder() {
    if !isolated(
        "agents::cached::tests::claude_installs_and_refreshes_plugins_loaded_from_a_folder",
    ) {
        return;
    }
    let mut fixture = Fixture::new(Agent::Claude);
    fixture.cache = fixture.state.join("local-plugin");
    fs::write(fixture.state.join("local"), "").unwrap();
    fs::write(
        fixture.state.join("installed.json"),
        serde_json::json!([{
            "id": "atlas@atlas", "scope": "user", "version": "0.1.0",
            "readFromFolder": fixture.cache
        }])
        .to_string(),
    )
    .unwrap();
    assert_eq!(fixture.install()["agents"][0]["changed"], true);
    fixture.current();
    assert_eq!(fixture.install()["agents"][0]["changed"], false);
    fixture.stale();
    assert_eq!(fixture.install()["agents"][0]["changed"], true);
    fixture.current();
    assert_eq!(fixture.install()["agents"][0]["changed"], false);
}

#[test]
fn update_refreshes_cached_contents_and_reports_hosts_that_leave_them_stale() {
    if !isolated(
        "agents::cached::tests::update_refreshes_cached_contents_and_reports_hosts_that_leave_them_stale",
    ) {
        return;
    }
    let fixtures: Vec<_> = [Agent::Claude, Agent::Codex]
        .into_iter()
        .map(|agent| {
            let fixture = Fixture::new(agent);
            assert_eq!(fixture.install()["ok"], true);
            fixture.stale();
            fixture
        })
        .collect();
    let refreshed = crate::agents::refresh();
    assert_eq!(refreshed["agents"].as_array().unwrap().len(), 2);
    for row in refreshed["agents"].as_array().unwrap() {
        assert_eq!(row["ok"], true, "{refreshed}");
    }
    for fixture in &fixtures {
        fixture.current();
        fixture.stale();
        fs::write(fixture.state.join("noop"), "").unwrap();
        let failed = fixture.install();
        assert_eq!(failed["ok"], false, "{failed}");
        assert!(failed["agents"][0]["changed"].is_null());
    }
    let failed = crate::agents::refresh();
    for row in failed["agents"].as_array().unwrap() {
        assert_eq!(row["ok"], false, "{failed}");
    }
}

#[test]
fn installation_rejects_noop_hosts_and_unreadable_lists_before_claiming_a_change() {
    if !isolated(
        "agents::cached::tests::installation_rejects_noop_hosts_and_unreadable_lists_before_claiming_a_change",
    ) {
        return;
    }
    for agent in [Agent::Claude, Agent::Codex] {
        let fixture = Fixture::new(agent);
        fs::write(fixture.state.join("noop"), "").unwrap();
        let failed = fixture.install();
        assert_eq!(failed["ok"], false, "{failed}");
        assert!(failed["agents"][0]["changed"].is_null());
        assert!(!fixture.cache.exists());

        fs::remove_file(fixture.state.join("noop")).unwrap();
        assert_eq!(fixture.install()["ok"], true);
        let before = fs::read(fixture.state.join("changes")).unwrap();
        fs::write(fixture.state.join("installed.json"), "not json").unwrap();
        assert_eq!(fixture.install()["ok"], false);
        assert_eq!(fs::read(fixture.state.join("changes")).unwrap(), before);
        fixture.current();
    }
}

#[test]
fn claude_only_compares_the_user_installation() {
    let list = serde_json::json!([
        {"id": "atlas@atlas", "scope": "project", "installPath": "/project/cache"},
        {"id": "atlas@atlas", "scope": "local", "readFromFolder": "/local/plugin"},
        {"id": "atlas@atlas", "scope": "user", "installPath": "/user/cache"}
    ]);
    assert_eq!(
        installed_in(Agent::Claude, &list)
            .unwrap()
            .map(|entry| entry.path),
        Some(PathBuf::from("/user/cache"))
    );
    for list in [
        serde_json::json!({}),
        serde_json::json!([{"id": "atlas@atlas", "scope": "user"}]),
        serde_json::json!([{"id": "atlas@atlas", "scope": "user", "installPath": "relative"}]),
    ] {
        assert!(installed_in(Agent::Claude, &list).is_err());
    }
}

#[test]
fn claude_compares_the_folder_it_reads_before_the_cached_install_path() {
    for install_path in [None, Some("/old/cache")] {
        let list = serde_json::json!([{
            "id": "atlas@atlas", "scope": "user",
            "readFromFolder": "/active/plugin", "installPath": install_path
        }]);
        assert_eq!(
            installed_in(Agent::Claude, &list)
                .unwrap()
                .map(|entry| entry.path),
            Some(PathBuf::from("/active/plugin"))
        );
    }
    for path in ["", "relative"] {
        let list = serde_json::json!([{
            "id": "atlas@atlas", "scope": "user", "readFromFolder": path,
            "installPath": "/old/cache"
        }]);
        assert!(installed_in(Agent::Claude, &list).is_err());
    }
}
