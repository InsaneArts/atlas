//! The desktops settings list, and which one a command uses.
//!
//! Settings that name one helper the old way, with the helper's fields at the
//! top, are one `user` desktop called `second-login`. Settings may instead
//! list desktops under `[[desktops]]` with a `default`; output placement and
//! what the activity keeps stay settings of the whole Mac.
use super::{Activity, Config, default_output, parse_toml};
use crate::desktop::{Desktop, DesktopId, RecordKey};
use crate::error::{Code, Error, Result};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// The name today's settings give their one desktop.
const SECOND_LOGIN: &str = "second-login";

/// The variable a person or an agent's host names a desktop with.
const NAME_VARIABLE: &str = "ATLAS_DESKTOP";

/// The variable an Atlas process tells its own Atlas children which desktop
/// it chose with, by record key, so neither a renamed entry nor a repaired
/// one redirects them.
const KEY_VARIABLE: &str = "ATLAS_DESKTOP_KEY";

/// The longest desktop name.
const NAME_MAX: usize = 32;

/// The helper's fields, which only a `user` desktop has.
const USER_FIELDS: [&str; 4] = [
    "helper_uid",
    "socket_path",
    "identity_probe",
    "input_socket",
];

/// Every desktop the settings list, in their order, and the one used when
/// nothing chooses another.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    default: String,
    desktops: Vec<Listed>,
}

/// One desktop of the settings: its name, a label only, and its settings
/// as a whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Listed {
    pub name: String,
    pub desktop: Chosen,
}

/// A desktop's settings, by kind: a Second Login's are its helper's
/// [`Config`]; a VM's, its identifier.
#[derive(Clone, Debug, PartialEq)]
pub enum Chosen {
    User(Config),
    Vm(VmConfig),
}

/// A VM desktop's settings, with those of the whole Mac.
#[derive(Clone, Debug, PartialEq)]
pub struct VmConfig {
    pub id: DesktopId,
    pub output_dir: PathBuf,
    pub activity: Activity,
}

/// What a client is built from: the desktop chosen, by the name the
/// settings give it, and the name of the Second Login they list, if any,
/// for advice that sends a person there.
#[derive(Clone, Debug, PartialEq)]
pub struct Selected {
    pub name: String,
    pub desktop: Chosen,
    pub user_entry: Option<String>,
}

impl Chosen {
    /// The desktop these settings name.
    pub fn desktop(&self) -> Desktop {
        match self {
            Self::User(config) => config.desktop(),
            Self::Vm(vm) => Desktop::from(crate::vm::Vm { id: vm.id.clone() }),
        }
    }

    /// Where the whole Mac keeps its runs.
    pub fn output_dir(&self) -> &Path {
        match self {
            Self::User(config) => &config.output_dir,
            Self::Vm(vm) => &vm.output_dir,
        }
    }

    /// Where this desktop's own records live: Second Login's at the root of
    /// the output folder, as before there were other desktops; any other's
    /// in a folder of its own beneath it, named by its record key.
    pub fn records(&self) -> PathBuf {
        match self {
            Self::User(config) => config.output_dir.clone(),
            Self::Vm(vm) => vm
                .output_dir
                .join("desktops")
                .join(self.desktop().record_key().as_str()),
        }
    }

    /// What the activity keeps.
    pub fn activity(&self) -> &Activity {
        match self {
            Self::User(config) => &config.activity,
            Self::Vm(vm) => &vm.activity,
        }
    }

    /// A Second Login's settings, when these are one's.
    pub fn user(&self) -> Option<&Config> {
        match self {
            Self::User(config) => Some(config),
            Self::Vm(_) => None,
        }
    }
}

impl Selected {
    /// Load the settings at `path` and take the desktop this process chose
    /// ([`choose_for_process`]), of whichever kind.
    ///
    /// # Errors
    ///
    /// As [`Config::load`], but a VM desktop is taken, not refused.
    pub fn load(path: &Path) -> Result<Self> {
        Config::load_settings(path)?.chosen()
    }

    /// The refusal of what a VM desktop cannot do from this build yet, naming
    /// the VM in one wording for every command and every surface. Its own
    /// next step, which the command line alone shows, stands in for the one
    /// its code would give (about this Mac) and names the Second Login to use
    /// instead when the settings list one.
    pub fn unreachable(&self) -> Error {
        let next = match &self.user_entry {
            Some(user) => format!("Run it again with --desktop {user} to use Second Login."),
            None => "Nothing was sent to it. A later Atlas reaches VM desktops.".to_owned(),
        };
        Error::new(
            Code::Platform,
            format!(
                "{} is a VM desktop, which this build of Atlas cannot reach yet.",
                self.name
            ),
        )
        .with("next", serde_json::json!(next))
    }

    /// A Second Login not yet saved in settings, as onboarding builds one
    /// before it writes them.
    pub fn provisional(config: Config) -> Self {
        Self {
            name: SECOND_LOGIN.to_owned(),
            desktop: Chosen::User(config),
            user_entry: None,
        }
    }

    /// The desktop chosen.
    pub fn desktop(&self) -> Desktop {
        self.desktop.desktop()
    }
}

/// What chooses the desktop of a command, strongest first; with nothing,
/// the default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// A name given with `--desktop`.
    pub flag: Option<String>,
    /// The desktop the Atlas process that started this one chose, by its
    /// record key as that process wrote it.
    pub parent: Option<String>,
    /// A name given with [`NAME_VARIABLE`].
    pub named: Option<String>,
    /// The desktop the agent session is bound to, by its record key.
    pub bound: Option<RecordKey>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListFile {
    default: String,
    desktops: Vec<Entry>,
    #[serde(default = "default_output")]
    output_dir: PathBuf,
    #[serde(default)]
    activity: Activity,
}

/// A desktop as the list writes it, by kind.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Entry {
    User {
        name: String,
        helper_uid: u32,
        socket_path: PathBuf,
        identity_probe: PathBuf,
        input_socket: PathBuf,
    },
    Vm {
        name: String,
        id: String,
    },
}

impl Settings {
    /// Load and validate the settings at `path`, with today's errors for a
    /// missing or unreadable file.
    ///
    /// # Errors
    ///
    /// As [`Config::load`].
    pub fn load(path: &Path) -> Result<Self> {
        Config::load_settings(path)
    }

    /// Parse settings text as if it were the file at `path`, in either form.
    ///
    /// # Errors
    ///
    /// Returns an error for text that is not valid settings: both forms at
    /// once, a duplicate or unusable name, a default the list lacks, or more
    /// than one `user` desktop.
    pub fn parse(bytes: &[u8], path: &Path) -> Result<Self> {
        let table: toml::Table = parse_toml(bytes, path)?;
        if !table.contains_key("desktops") {
            return Ok(Self::one(Config::parse(bytes, path)?));
        }
        if let Some(field) = USER_FIELDS.iter().find(|field| table.contains_key(**field)) {
            return Err(invalid(format!(
                "{} lists desktops, so {field} belongs in its desktop's entry",
                super::tilde(path)
            )));
        }
        if !table.contains_key("default") {
            return Err(invalid(format!(
                "{} lists desktops but names no default",
                super::tilde(path)
            )));
        }

        let file: ListFile = parse_toml(bytes, path)?;
        let mut desktops = Vec::with_capacity(file.desktops.len());
        for entry in file.desktops {
            match entry {
                Entry::User {
                    name,
                    helper_uid,
                    socket_path,
                    identity_probe,
                    input_socket,
                } => {
                    let config = Config {
                        helper_uid,
                        socket_path,
                        identity_probe,
                        input_socket,
                        output_dir: file.output_dir.clone(),
                        activity: file.activity.clone(),
                    };
                    desktops.push(Listed {
                        name,
                        desktop: Chosen::User(config.settled(path)?),
                    });
                }
                Entry::Vm { name, id } => {
                    let Some(id) = DesktopId::parse(&id) else {
                        return Err(invalid(format!(
                            "\"{id}\" is not a VM's identifier: Atlas VM gives each one 24 \
                             lowercase hexadecimal digits"
                        )));
                    };
                    desktops.push(Listed {
                        name,
                        desktop: Chosen::Vm(VmConfig {
                            id,
                            output_dir: Config::settled_shared(
                                &file.output_dir,
                                &file.activity,
                                path,
                            )?,
                            activity: file.activity.clone(),
                        }),
                    });
                }
            }
        }
        let settings = Self {
            default: file.default,
            desktops,
        };
        settings.validate()?;
        Ok(settings)
    }

    /// Settings of the old form: `config` alone, as `second-login`.
    fn one(config: Config) -> Self {
        Self {
            default: SECOND_LOGIN.to_owned(),
            desktops: vec![Listed {
                name: SECOND_LOGIN.to_owned(),
                desktop: Chosen::User(config),
            }],
        }
    }

    fn validate(&self) -> Result<()> {
        if self.desktops.is_empty() {
            return Err(invalid("The settings list no desktops"));
        }
        let mut users = 0;
        for (index, listed) in self.desktops.iter().enumerate() {
            if !plain_name(&listed.name) {
                return Err(invalid(format!(
                    "\"{}\" is not a desktop name: use 1 to {NAME_MAX} lowercase letters, \
                     digits and dashes",
                    listed.name
                )));
            }
            if self.desktops[..index]
                .iter()
                .any(|earlier| earlier.name == listed.name)
            {
                return Err(invalid(format!(
                    "The settings list the desktop \"{}\" twice",
                    listed.name
                )));
            }
            if listed.desktop.user().is_some() {
                users += 1;
                if users > 1 {
                    return Err(invalid("The settings list more than one Second Login"));
                }
            }
            // Two entries naming one desktop would share its locks and
            // records, whatever their kinds and order.
            let id = listed.desktop.desktop().id();
            if self.desktops[..index]
                .iter()
                .any(|earlier| earlier.desktop.desktop().id() == id)
            {
                return Err(invalid(format!(
                    "The settings list the desktop {} twice",
                    id.as_str()
                )));
            }
        }
        if self.named(&self.default).is_none() {
            return Err(invalid(format!(
                "The default desktop \"{}\" is not in the list",
                self.default
            )));
        }
        Ok(())
    }

    /// Every desktop, in the settings' order.
    pub fn desktops(&self) -> &[Listed] {
        &self.desktops
    }

    /// The desktop used when nothing chooses another.
    pub fn default_desktop(&self) -> &Listed {
        // Validation keeps the default in the list.
        self.named(&self.default).unwrap_or(&self.desktops[0])
    }

    fn named(&self, name: &str) -> Option<&Listed> {
        self.desktops.iter().find(|listed| listed.name == name)
    }

    /// The desktop whose records carry `key`.
    fn keyed(&self, key: &RecordKey) -> Option<&Listed> {
        self.desktops
            .iter()
            .find(|listed| listed.desktop.desktop().record_key() == *key)
    }

    /// The desktop `selection` chooses.
    ///
    /// # Errors
    ///
    /// Refuses a name, a parent's desktop or a binding the settings no
    /// longer list: nothing falls back to another desktop.
    pub fn resolve(&self, selection: &Selection) -> Result<&Listed> {
        if let Some(name) = &selection.flag {
            return self.named(name).ok_or_else(|| unlisted(name));
        }
        if let Some(key) = &selection.parent {
            let key = RecordKey::parse(key).ok_or_else(|| {
                Error::new(
                    Code::ConfigInvalid,
                    format!("\"{key}\" is not a desktop this command can be started for"),
                )
            })?;
            return self.keyed(&key).ok_or_else(|| {
                Error::new(
                    Code::ConfigInvalid,
                    "The desktop this command was started for is no longer in the settings",
                )
            });
        }
        if let Some(name) = &selection.named {
            return self.named(name).ok_or_else(|| unlisted(name));
        }
        if let Some(key) = &selection.bound {
            return self.keyed(key).ok_or_else(|| {
                Error::new(
                    Code::ConfigInvalid,
                    "This session's desktop is no longer in the settings. Name another with \
                     --desktop, or start a new session.",
                )
            });
        }
        Ok(self.default_desktop())
    }

    /// The settings of the desktop this process chose, or, when it chose
    /// none, of the one its environment names.
    ///
    /// # Errors
    ///
    /// Returns the failure of this process's choice, and refuses a chosen
    /// desktop the settings no longer list.
    pub(super) fn chosen(&self) -> Result<Selected> {
        let listed = match CHOSEN.get() {
            Some(Ok(key)) => self.keyed(key).ok_or_else(|| {
                Error::new(
                    Code::ConfigInvalid,
                    "The desktop this command chose is no longer in the settings",
                )
            })?,
            Some(Err((code, message))) => return Err(Error::new(*code, message.clone())),
            None => self.resolve(&Selection::from_env(None))?,
        };
        Ok(self.selected(listed))
    }

    /// `listed` as a client is built from it.
    pub fn selected(&self, listed: &Listed) -> Selected {
        Selected {
            name: listed.name.clone(),
            desktop: listed.desktop.clone(),
            user_entry: self
                .desktops
                .iter()
                .find(|listed| listed.desktop.user().is_some())
                .map(|listed| listed.name.clone()),
        }
    }

    /// The `user` desktop: its own setup, update and removal work on it,
    /// whichever desktop is the default.
    ///
    /// # Errors
    ///
    /// Refuses settings with no Second Login.
    pub fn user(&self) -> Result<Config> {
        self.desktops
            .iter()
            .find_map(|listed| listed.desktop.user())
            .cloned()
            .ok_or_else(|| Error::new(Code::Config, "The settings list no Second Login"))
    }
}

impl Selection {
    /// What this process's environment chooses, with `flag` given on its
    /// command line.
    pub fn from_env(flag: Option<String>) -> Self {
        let variable = |name| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            flag,
            parent: variable(KEY_VARIABLE),
            named: variable(NAME_VARIABLE),
            bound: None,
        }
    }
}

/// What this process chose: a desktop by its record key, or why it could
/// not choose.
static CHOSEN: OnceLock<std::result::Result<RecordKey, (Code, String)>> = OnceLock::new();

/// What this process's command line and environment named, for a session
/// found later to be chosen for with the same names.
static SELECTED: OnceLock<Selection> = OnceLock::new();

/// Keep this process's choice for good, made with `selection`: every later
/// [`Config::load`] takes that desktop's current entry, or refuses with the
/// failure. The first choice stays.
pub fn choose_for_process(selection: Selection, choice: Result<RecordKey>) {
    let _ = SELECTED.set(selection);
    let _ = CHOSEN.set(choice.map_err(|error| (error.code, error.error)));
}

/// What this process's desktop was chosen with, or, when it chose none, what
/// its environment names.
pub fn process_selection() -> Selection {
    SELECTED
        .get()
        .cloned()
        .unwrap_or_else(|| Selection::from_env(None))
}

/// The variable and value that give an Atlas child this process's desktop,
/// when it chose one.
pub fn for_children() -> Option<(&'static str, String)> {
    match CHOSEN.get() {
        Some(Ok(key)) => Some((KEY_VARIABLE, key.as_str().to_owned())),
        _ => None,
    }
}

/// Whether `name` is a desktop name: lowercase letters, digits and dashes.
fn plain_name(name: &str) -> bool {
    (1..=NAME_MAX).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn unlisted(name: &str) -> Error {
    Error::new(
        Code::ConfigInvalid,
        format!(
            "The settings list no desktop named \"{name}\". Name one they list, or unset \
             ATLAS_DESKTOP."
        ),
    )
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(Code::Config, message)
}

impl Listed {
    /// The name this desktop's records carry.
    pub fn record_key(&self) -> RecordKey {
        self.desktop.desktop().record_key()
    }
}

/// The settings text `text`, of the file at `path`, with its Second Login
/// replaced by `user`'s helper. Settings that name one helper become
/// `user`'s whole; a list keeps every other desktop, its order, the
/// station's settings and every comment, and only the Second Login's
/// helper fields change.
///
/// # Errors
///
/// Returns an error for text that is not valid settings, a list with no
/// Second Login, or a result that would not load.
pub fn with_user(text: &str, path: &Path, user: &Config) -> Result<String> {
    Settings::parse(text.as_bytes(), path)?;
    if !text_lists(text, path)? {
        return user.to_toml();
    }
    let mut document = text.parse::<toml_edit::DocumentMut>().map_err(|error| {
        super::unparsed(&super::tilde(path), text, error.span(), super::NOT_TOML)
    })?;
    // The list may be written as tables or inline, and its Second Login's
    // helper fields change in place either way.
    let path_of = |value: &Path| toml_edit::Value::from(value.to_string_lossy().into_owned());
    let set = |entry: &mut dyn toml_edit::TableLike| {
        put(entry, "helper_uid", i64::from(user.helper_uid).into());
        put(entry, "socket_path", path_of(&user.socket_path));
        put(entry, "identity_probe", path_of(&user.identity_probe));
        put(entry, "input_socket", path_of(&user.input_socket));
    };
    let is_user = |entry: &dyn toml_edit::TableLike| {
        entry.get("kind").and_then(toml_edit::Item::as_str) == Some("user")
    };
    let desktops = document.get_mut("desktops");
    let found = match desktops {
        Some(toml_edit::Item::ArrayOfTables(tables)) => tables
            .iter_mut()
            .find(|entry| is_user(&**entry))
            .map(|entry| set(entry))
            .is_some(),
        Some(toml_edit::Item::Value(toml_edit::Value::Array(array))) => array
            .iter_mut()
            .filter_map(toml_edit::Value::as_inline_table_mut)
            .find(|entry| is_user(&**entry))
            .map(|entry| set(entry))
            .is_some(),
        _ => false,
    };
    if !found {
        return Err(invalid("The settings list no Second Login"));
    }
    let written = document.to_string();
    if written.len() as u64 > super::LIMIT {
        return Err(invalid("Configuration exceeds 16 KiB"));
    }
    Settings::parse(written.as_bytes(), path)?;
    Ok(written)
}

/// Set `key` of `entry` to `value`, keeping the comments and spacing the
/// value it replaces had.
fn put(entry: &mut dyn toml_edit::TableLike, key: &str, mut value: toml_edit::Value) {
    match entry.get_mut(key).and_then(toml_edit::Item::as_value_mut) {
        Some(written) => {
            *value.decor_mut() = written.decor().clone();
            *written = value;
        }
        None => {
            entry.insert(key, toml_edit::Item::Value(value));
        }
    }
}

/// Whether settings text lists desktops rather than naming one helper.
fn text_lists(text: &str, path: &Path) -> Result<bool> {
    let table: toml::Table = parse_toml(text.as_bytes(), path)?;
    Ok(table.contains_key("desktops"))
}

#[cfg(test)]
#[path = "desktops_tests.rs"]
mod tests;
