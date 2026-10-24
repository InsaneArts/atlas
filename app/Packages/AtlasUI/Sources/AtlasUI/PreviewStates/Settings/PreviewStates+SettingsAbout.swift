#if DEBUG
  extension PreviewStates {
    /// A check for updates under way: the button waits and the line under it says so.
    @MainActor static let settingsAboutChecking: SettingsState = about(.checking, .working("Checking for updates…"))

    /// The check found nothing newer: a green check pops in with the words.
    @MainActor static let settingsAboutUpToDate: SettingsState = about(.check, .done("Atlas is up to date."))

    /// The check found a newer version: Install Update… turns blue and asks first.
    @MainActor static let settingsAboutNewer: SettingsState = about(.install, .available("Atlas 0.2.0 is available."))

    /// The check did not answer.
    @MainActor static let settingsAboutFailed: SettingsState = about(
      .check,
      .problem("Atlas couldn't check for updates.")
    )

    /// The newer version installing: the button waits, plain, over one progress bar and its line.
    @MainActor static let settingsAboutInstalling: SettingsState = about(
      .installing,
      .installing("Installing Atlas 0.2.0…")
    )

    /// The install failed because Atlas isn't in the Applications folder: what to do, and Show in Finder.
    @MainActor static let settingsAboutInstallFailed: SettingsState = about(
      .check,
      .misplaced("Move Atlas to the Applications folder, then try again.")
    )

    /// No release is published yet: plain words, not a failure, with the site as a link.
    @MainActor static let settingsAboutUnpublished: SettingsState = about(
      .check,
      .note("Atlas has no public release yet. Check [atlas.insanearts.io](https://atlas.insanearts.io) for news.")
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
