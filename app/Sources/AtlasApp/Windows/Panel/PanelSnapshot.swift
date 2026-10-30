import AtlasService
import AtlasUI
import Foundation

/// What the panel's controller holds, as one plain value for ``PanelPresenter``.
struct PanelSnapshot {
  /// The latest status the service reported.
  var status = ViewerStatus()
  /// Whether the panel waits for the service's answer, before the first one or after a reading that failed, so it
  /// says it is connecting.
  var connecting = false
  /// Whether answers came before and the last reading failed, so connecting reads as Reconnecting….
  var reconnecting = false
  /// The helper's screen, when the last capture gave one.
  var picture: DesktopPicture?
  /// What the panel is doing: paused, an action of its own under way, its notice and its ⋯ menu.
  var session = PanelSession()
  /// Whether Atlas's main window holds control now, which reads as You have control and offers Give Back; control
  /// another window holds reads as In use elsewhere.
  var holdingControl = false
  /// The helper's commands running now, from any window, so the buttons and items that start one wait.
  var running: Set<HelperActions.Command> = []
  /// Whether this process's graphical session is on this Mac's console.
  var isOnConsole = false
  /// Stop and Resume requests unanswered from any window, so Resume waits.
  var agentRequests = 0
  /// Where Open at Login stands; nil when unknown, as in a render.
  var opensAtLogin: OpenAtLogin.State?
  /// The time it was taken, which a session's age counts to.
  var date = Date()
  /// The icons found for Recent's apps, by their key's text (``AppIconKey/text``).
  var icons: [String: IconImage] = [:]
}
