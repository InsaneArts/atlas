#if DEBUG
  extension PreviewStates {
    /// A check for updates under way: a spinner in the button.
    @MainActor static let settingsAboutChecking: SettingsState = about(.checking)

    /// The check found nothing newer: a green check pops in with the words.
    @MainActor static let settingsAboutUpToDate: SettingsState = about(.check, .done("Atlas is up to date."))

    /// The check found a newer version: Install Update… turns blue and asks first.
    @MainActor static let settingsAboutNewer: SettingsState = about(.install, .available("Atlas 0.2.0 is available."))

    /// The check did not answer.
    @MainActor static let settingsAboutFailed: SettingsState = about(
      .check,
      .problem("Atlas couldn't check for updates.")
    )

    /// The newer version installing: a spinner in the button and a progress bar where the status goes.
    @MainActor static let settingsAboutInstalling: SettingsState = about(
      .installing,
      .installing("Installing Atlas 0.2.0…")
    )

    /// The install failed with a long reason, which wraps under the buttons in the problem style.
    @MainActor static let settingsAboutInstallFailed: SettingsState = about(
      .check,
      .problem(
        "Atlas.app must be where the helper account can run it: install it in /Applications, then check for updates "
          + "again. The release was downloaded and its signature checked, but nothing was replaced."
      )
    )

    /// No release is published yet: plain small print, not a failure.
    @MainActor static let settingsAboutUnpublished: SettingsState = about(
      .check,
      .note("Atlas has no public release yet. Check atlas.insanearts.io for news.")
    )

    /// The About pane with its button and status.
    ///
    /// - Parameters:
    ///   - update: The update button.
    ///   - status: What it says about updates; nothing by default.
    /// - Returns: The sample.
    @MainActor private static func about(
      _ update: SettingsUpdateButton,
      _ status: SettingsUpdateStatus? = nil
    ) -> SettingsState {
      var state = selecting(.about)
      state.about.update = update
      state.about.status = status
      return state
    }
  }
#endif
