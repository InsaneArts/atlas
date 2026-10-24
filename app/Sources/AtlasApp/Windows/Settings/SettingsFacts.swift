import AtlasService

/// What Settings reads from the rest of the app each time it draws: the helper's status, Open at Login, the picture,
/// the helper's commands running and the app's version.
struct SettingsFacts {
  /// The helper's latest status.
  var status = ViewerStatus()
  /// Whether no status reading has answered yet.
  var connecting = true
  /// Where Open at Login stands.
  var opensAtLogin = OpenAtLogin.State(isAvailable: false, isEnabled: false)
  /// The picture choice in force.
  var picture = ViewerPictureChoice.live
  /// Whether Paused can be chosen now.
  var canPause = false
  /// The helper's commands running now, from any window.
  var running: Set<HelperActions.Command> = []
  /// The agents being connected now, from any window, by id.
  var installing: Set<String> = []
  /// The app's version, when known.
  var version: String?
}
