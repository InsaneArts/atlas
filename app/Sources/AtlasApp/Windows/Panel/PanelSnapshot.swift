import AtlasKit
import AtlasUI

/// What the panel's controller holds, as one plain value for ``PanelPresenter``.
struct PanelSnapshot {
  /// The latest status the service reported.
  var status = ViewerStatus()
  /// The helper's desktop, when the last capture gave one.
  var picture: DesktopPicture?
  /// Why there is no desktop to show.
  var placeholder = PanelWords.waitingForPicture
  /// What the last action said.
  var feedback: String?
  /// Whether an action still runs, so every button that starts one waits and the feedback's spinner shows.
  var working = false
  /// The helper's short name: the service's account, else the one `atlas switch --list` reported.
  var helper: String?
  /// Where Open at Login stands; nil when unknown, as in a render.
  var opensAtLogin: OpenAtLogin.State?
  /// Where logging out stands.
  var logOutStage = LogOutStage.idle
}
