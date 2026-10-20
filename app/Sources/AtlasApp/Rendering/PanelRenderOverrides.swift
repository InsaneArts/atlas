import AtlasKit
import AtlasUI

/// What `--render` changes in the live status for design checks, read from its environment:
/// `ATLAS_RENDER_MODE` sets the mode, `ATLAS_RENDER_MESSAGE` shows a feedback line, `ATLAS_RENDER_LOGOUT` (`ask` or
/// `held`) shows the log-out confirmation or the offer to force one, and `ATLAS_RENDER_PREVIEW=1` shows every action
/// as on a set-up Mac, as any `ATLAS_RENDER_LOGOUT` does.
struct PanelRenderOverrides: Equatable, Sendable {
  /// The mode to show in place of the helper's; nil keeps it.
  var mode: HelperMode?
  /// A feedback line to show; nil shows none.
  var message: String?
  /// The log-out step to show.
  var logOutStage: LogOutStage
  /// Whether every action shows as on a set-up Mac: a known helper and Open at Login.
  var showsEverything: Bool

  /// Reads the overrides from an environment.
  ///
  /// - Parameter environment: The process's environment variables.
  init(environment: [String: String]) {
    mode = environment["ATLAS_RENDER_MODE"].flatMap(HelperMode.init(rawValue:))
    message = environment["ATLAS_RENDER_MESSAGE"].flatMap { $0.isEmpty ? nil : $0 }
    let logOut = environment["ATLAS_RENDER_LOGOUT"]
    logOutStage =
      switch logOut {
      case "ask": .confirming
      case "held": .held
      default: .idle
      }
    showsEverything = logOut != nil || environment["ATLAS_RENDER_PREVIEW"] == "1"
  }

  /// A snapshot with these overrides applied.
  ///
  /// - Parameter snapshot: What the live service showed.
  /// - Returns: The snapshot to draw.
  func applied(to snapshot: PanelSnapshot) -> PanelSnapshot {
    var shown = snapshot
    if let mode { shown.status.overview.mode = mode }
    shown.feedback = message
    shown.logOutStage = logOutStage
    if showsEverything {
      shown.helper = "helper"
      shown.opensAtLogin = OpenAtLogin.State(isAvailable: true, isEnabled: false)
    }
    return shown
  }
}
