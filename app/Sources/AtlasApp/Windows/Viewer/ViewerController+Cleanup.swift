import AtlasService
import AtlasUI

/// Clean Up This Session's Work…: reads what cleaning up the session's work takes, which changes nothing, then asks
/// through ``ask(_:)``, so its Clean Up runs once however often it is pressed. Clean Up cleans up as the person; what
/// each step did stays above the button, and the sessions' list is read again. The button waits while either request
/// runs.
extension ViewerController {
  /// Reads what cleaning up `key`'s work takes, then asks before it runs; with nothing to clean up, says so instead.
  /// A press while a cleanup is read or runs, or while an alert is up, does nothing.
  ///
  /// - Parameter key: The session's key.
  func askCleanup(_ key: String) async {
    guard session.cleaningUp == nil, session.alert == nil else { return }
    guard !session.alertRunning, !session.recovering else { return say(HelperWords.waitForAction) }
    session.cleaningUp = key
    let preview: CleanupReply
    do {
      preview = try await service.cleanup(session: key, apply: false)
    } catch {
      session.cleaningUp = nil
      if !error.isCancelled { say(error.message) }
      return
    }
    session.cleaningUp = nil
    guard !preview.isEmpty else {
      say(ViewerWords.nothingToCleanUp)
      return panesModel.followSessions(readNow: true)
    }
    ask(
      .cleanup(
        session: key,
        steps: ViewerPresenter.cleanupSteps(preview),
        warning: ViewerPresenter.cleanupWarning(preview)
      )
    )
  }

  /// Cleans up `key`'s work, once the person pressed Clean Up: the alert closes with a notice, what each step did
  /// stays for Details, and the sessions' list is read again. One that ran and did not finish says so, and its
  /// steps still show.
  ///
  /// - Parameter key: The session's key.
  func cleanUp(_ key: String) async {
    session.cleaningUp = key
    do {
      let reply = try await service.cleanup(session: key, apply: true)
      session.cleanupReceipts[key] = reply.isEmpty ? nil : reply
      say(ViewerPresenter.cleanupNotice(reply))
    } catch {
      say(error.message)
    }
    session.alert = nil
    session.cleaningUp = nil
    panesModel.followSessions(readNow: true)
  }
}
