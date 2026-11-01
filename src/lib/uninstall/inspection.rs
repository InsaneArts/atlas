use super::*;

/// What uninstall finds before it asks.
pub(super) struct Found {
    pub(super) app: PathBuf,
    pub(super) executable: PathBuf,
    /// For the settings, when they load.
    pub(super) client: Option<Client>,
    /// Why settings that exist do not load: they name no helper, so its
    /// job stays.
    pub(super) unloaded: Option<String>,
    /// The helper account's name, when it still exists and is not known
    /// to be another account.
    pub(super) helper: Option<String>,
    /// Whether the account with the helper's user ID is the one recorded;
    /// `None` without settings or an account.
    pub(super) known: Option<Known>,
    pub(super) data: PathBuf,
    pub(super) leftovers: Vec<PathBuf>,
}

/// Find what uninstall would remove and what it keeps, changing nothing.
pub(super) fn survey(config_path: &Path) -> Result<Found> {
    let app = crate::update::installed_app()?;
    let executable = crate::update::installed_executable()?;

    // Settings that exist but do not load name no helper, so its job stays.
    let (client, unloaded) = match Config::load(config_path) {
        Ok(config) => (Some(Client::new(config)?), None),
        Err(error) if error.code == Code::ConfigInvalid => (None, Some(error.error)),
        Err(_) => (None, None),
    };

    let config = client.as_ref().map(|client| &client.config);
    let account =
        config.and_then(|config| setup::account_with_uid(config.helper_uid).ok().flatten());
    let known = account
        .as_ref()
        .map(|account| known(setup::identity::standing_of(account)));
    let helper = account
        .filter(|_| known != Some(Known::Other))
        .map(|account| account.name);
    let data = config::home()?.join("Library/Application Support/Atlas");
    let leftovers = config
        .map(|config| handed_over(&config.identity_probe))
        .unwrap_or_default();

    Ok(Found {
        app,
        executable,
        client,
        unloaded,
        helper,
        known,
        data,
        leftovers,
    })
}
