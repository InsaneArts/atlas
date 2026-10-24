import AtlasUI
import Foundation

extension SettingsPresenter {
  /// The alert over the whole window, if one is asked. The helper's alerts read as they do in every window
  /// (``HelperAlert/state(overview:progress:)``); Forget and Install are Settings' own. Recover shows its refusal
  /// and its success in place.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The alert, or nil for none.
  static func alert(_ snapshot: SettingsSnapshot) -> AlertCardState<AlertChoice>? {
    let session = snapshot.session
    guard let kind = session.alert else { return nil }
    let overview = snapshot.facts.status.overview
    let busy = session.alertRunning || (kind == .recover && snapshot.facts.running.contains(.recovery))
    let progress = AlertProgress.of(running: busy, failure: session.alertRefusal, done: session.alertDone)
    return switch kind {
    case .recover:
      HelperAlert.recover
        .state(overview: overview, progress: progress)
    case .switchToHelper:
      HelperAlert.switchToHelper
        .state(overview: overview, progress: progress)
    case .logOut:
      HelperAlert.logOut
        .state(overview: overview, progress: progress)
    case .logOutAnyway:
      HelperAlert.logOutAnyway
        .state(overview: overview, progress: progress)
    case .forget(let path):
      HelperAlert.destructive(
        SettingsWords.forgetTitle(projectName(path)),
        SettingsWords.forgetMessage,
        SettingsWords.forget,
        phase: progress.phase(running: SettingsWords.forgetting, done: SettingsWords.forget)
      )
    case .install(let version):
      HelperAlert.plain(
        SettingsWords.installTitle(version),
        SettingsWords.installMessage,
        SettingsWords.install,
        cancel: HelperWords.notNow,
        phase: progress.phase(running: SettingsWords.installing, done: SettingsWords.install)
      )
    }
  }
}
