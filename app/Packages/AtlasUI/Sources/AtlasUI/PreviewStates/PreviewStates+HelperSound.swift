#if DEBUG
  extension PreviewStates {
    /// The helper sound button muted, on, and unable to keep the helper silent.
    public static let helperSound = [
      HelperSoundButton(isMuted: true),
      HelperSoundButton(isMuted: false),
      HelperSoundButton(
        isMuted: true,
        problem:
          "Atlas needs permission to keep the helper's apps silent. Switch to the helper and allow Atlas when macOS asks."
      ),
    ]

    /// The panel working, its helper's sound turned on.
    @MainActor static var panelSoundOn: PanelState {
      var state = panelWorking
      state.picture.sound = helperSound[1]
      return state
    }

    /// The panel working, its helper not able to stay silent.
    @MainActor static var panelSoundUnavailable: PanelState {
      var state = panelWorking
      state.picture.sound = helperSound[2]
      return state
    }

    /// The main window working, its helper's sound turned on.
    @MainActor static var viewerSoundOn: ViewerState {
      var state = viewerWorking
      state.footer.controls.sound = helperSound[1]
      return state
    }

    /// The main window working, its helper not able to stay silent.
    @MainActor static var viewerSoundUnavailable: ViewerState {
      var state = viewerWorking
      state.footer.controls.sound = helperSound[2]
      return state
    }
  }
#endif
