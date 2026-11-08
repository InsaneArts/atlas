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
    let busy = session.alertRunning || (kind == .helper(.recover) && snapshot.facts.running.contains(.recovery))
    let progress = AlertProgress.of(running: busy, failure: session.alertRefusal, done: session.alertDone)
    return switch kind {
    case .helper(let alert):
      alert.state(overview: overview, progress: progress, askedByPerson: true, onConsole: snapshot.facts.isOnConsole)
    case .forget(let path):
      HelperAlert.destructive(
        SettingsWords.forgetTitle(projectName(path)),
        SettingsWords.forgetMessage,
        SettingsWords.forget,
        phase: progress.phase(
          running: SettingsWords.forgetting,
          done: SettingsWords.forget,
          doneMessage: SettingsWords.forgetMessage
        )
      )
    case .openHome:
      HelperAlert.destructive(
        SettingsWords.openHomeTitle,
        SettingsWords.openHomeMessage,
        SettingsWords.letItOpen,
        cancel: SettingsWords.keepPrivate,
        phase: progress.phase(
          running: SettingsWords.changingFiles,
          done: SettingsWords.letItOpen,
          doneMessage: SettingsWords.openHomeMessage
        )
      )
    case .allowSignIns:
      HelperAlert.destructive(
        SettingsWords.allowSignInsTitle,
        SettingsWords.allowSignInsMessage,
        SettingsWords.allow,
        cancel: SettingsWords.dontAllow,
        phase: progress.phase(
          running: SettingsWords.changingFiles,
          done: SettingsWords.allow,
          doneMessage: SettingsWords.allowSignInsMessage
        )
      )
    case .deletePictures:
      HelperAlert.destructive(
        SettingsWords.deletePicturesTitle,
        SettingsWords.deletePicturesMessage(snapshot.pictures.stored?.count),
        SettingsWords.deleteAll,
        phase: progress.phase(
          running: SettingsWords.deleting,
          done: SettingsWords.deleteAll,
          doneMessage: SettingsWords.deletePicturesMessage(nil)
        )
      )
    case .install(let version):
      HelperAlert.plain(
        SettingsWords.installTitle(version),
        SettingsWords.installMessage,
        SettingsWords.install,
        cancel: HelperWords.cancel,
        phase: progress.phase(
          running: SettingsWords.installing,
          done: SettingsWords.install,
          doneMessage: SettingsWords.installMessage
        )
      )
    }
  }
}
