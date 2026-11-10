#if DEBUG
  extension PreviewStates {
    /// Your files: the home folder is private from the helper, and sign-in copies stay out.
    static let settingsFilesPrivate = SettingsFilesState(
      home: SettingsFilesRow(
        isPrivate: true,
        title: "Your home folder is private from the helper",
        detail: "Agents still open your builds: Atlas copies them in first",
        button: "Undo…",
        heldButton: "Make Private",
        canRun: true
      ),
      signInCopies: false,
      canChangeSignIns: true
    )

    /// Your files: the helper can open six of the person's folders, and Make Private is offered.
    static let settingsFilesOpen = SettingsFilesState(
      home: SettingsFilesRow(
        isPrivate: false,
        title: "The helper can open 6 of your folders",
        detail: "Such as dev and .config",
        button: "Make Private",
        canRun: true
      ),
      signInCopies: false,
      canChangeSignIns: true
    )

    /// Your files: Make Private runs, a spinner in its button's place.
    static let settingsFilesWorking: SettingsFilesState = {
      var state = settingsFilesOpen
      state.home?.canRun = false
      state.home?.isWorking = true
      state.canChangeSignIns = false
      return state
    }()

    /// Your files: Atlas couldn't read how the home folder stands, so neither row shows, only why.
    static let settingsFilesUnread = SettingsFilesState(
      home: nil,
      signInCopies: nil,
      canChangeSignIns: true,
      status: SettingsNote(text: "Atlas couldn't read your home folder's permissions.", kind: .problem)
    )

    /// Your files: making the home folder private failed, saying the Terminal line that does it by hand.
    static let settingsFilesFailed: SettingsFilesState = {
      var state = settingsFilesOpen
      state.status = SettingsNote(
        text:
          "Atlas couldn't make your home folder private. In Terminal, run: chmod +a# 0 \"user:atlas deny list,search\" ~",
        kind: .problem
      )
      return state
    }()

    /// Your files: the person let agents copy sign-ins into the helper.
    static let settingsFilesCopiesAllowed: SettingsFilesState = {
      var state = settingsFilesPrivate
      state.signInCopies = true
      return state
    }()

    /// The Privacy page with the home folder open to the helper.
    @MainActor static let settingsHelperHomeOpen: SettingsState = {
      var state = selecting(.privacy)
      state.privacy = settingsFilesOpen
      return state
    }()

    /// The Privacy page after making the home folder private failed.
    @MainActor static let settingsHelperHomeFailed: SettingsState = {
      var state = selecting(.privacy)
      state.privacy = settingsFilesFailed
      return state
    }()

    /// The Privacy page with sign-in copies allowed.
    @MainActor static let settingsHelperCopiesAllowed: SettingsState = {
      var state = selecting(.privacy)
      state.privacy = settingsFilesCopiesAllowed
      return state
    }()

    /// The Privacy page when Atlas couldn't read the person's files.
    @MainActor static let settingsPrivacyUnread: SettingsState = {
      var state = selecting(.privacy)
      state.privacy = settingsFilesUnread
      return state
    }()

    /// Undo… asked: Keep Private is the default.
    @MainActor static let settingsOpenHome: SettingsState = {
      var state = selecting(.privacy)
      state.alert = AlertCardState(
        title: "Let the helper open your home folder again?",
        message: "Agents working in the helper could read the folders you made, such as your projects.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Keep Private", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Let It Open", role: .destructive),
        ]
      )
      return state
    }()

    /// Copying sign-ins asked: Don't Allow is the default.
    @MainActor static let settingsAllowSignIns: SettingsState = {
      var state = selecting(.privacy)
      state.alert = AlertCardState(
        title: "Let agents copy your sign-ins into the helper?",
        message: "An agent could sign the helper in to your accounts without you, by copying a browser's cookies.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Don't Allow", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Allow", role: .destructive),
        ]
      )
      return state
    }()
  }
#endif
