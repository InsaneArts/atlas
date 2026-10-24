import AtlasUI
import Foundation

extension SettingsPresenter {
  /// The alert over the whole window, if one is asked. The helper's alerts read as they do in every window
  /// (``HelperAlert/state(overview:busy:failure:)``); Forget and Install are Settings' own.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The alert, or nil for none.
  static func alert(_ snapshot: SettingsSnapshot) -> OrbitAlertState<AlertChoice>? {
    guard let kind = snapshot.session.alert else { return nil }
    let overview = snapshot.facts.status.overview
    let busy = snapshot.session.alertRunning || (kind == .recover && snapshot.facts.running.contains(.recovery))
    return switch kind {
    case .recover:
      HelperAlert.recover
        .state(overview: overview, busy: busy)
    case .switchToHelper:
      HelperAlert.switchToHelper
        .state(overview: overview, busy: busy)
    case .logOut:
      HelperAlert.logOut
        .state(overview: overview, busy: busy)
    case .logOutAnyway:
      HelperAlert.logOutAnyway
        .state(overview: overview, busy: busy)
    case .forget(let path):
      HelperAlert.destructive(
        SettingsWords.forgetTitle(projectName(path)),
        SettingsWords.forgetMessage,
        busy ? SettingsWords.forgetting : SettingsWords.forget,
        busy: busy
      )
    case .install(let version):
      HelperAlert.plain(
        SettingsWords.installTitle(version),
        SettingsWords.installMessage,
        busy ? SettingsWords.installing : SettingsWords.install,
        cancel: ViewerWords.notNow,
        busy: busy
      )
    }
  }
}
