import AtlasService
import AtlasUI
import Foundation

/// Clean Up This Session's Work…: reads what cleaning up the session's work takes, which changes nothing, then asks
/// through ``ask(_:)``, so its Clean Up runs once however often it is pressed. Clean Up cleans up as the person; what
/// each step did stays above the button, and the sessions' list is read again. The button waits while either request
/// runs, and while a cleanup whose reply never came may still run.
extension ViewerController {
  /// 5 min: how long Clean Up waits on a cleanup whose reply never came while `cleanup.json` still shows a step under
  /// way. Well past the longest cleanup, so a step that a run cut short left started does not hold the button for
  /// good.
  static let cleanupWaitLimit: TimeInterval = 300

  /// Reads what cleaning up `key`'s work takes, then asks before it runs; with nothing to clean up, says so instead.
  /// A press while a cleanup is read, runs or may still run, or while an alert is up, does nothing; so does a preview
  /// that comes back once another session is chosen or an alert went up meanwhile.
  ///
  /// - Parameter key: The session's key.
  func askCleanup(_ key: String) async {
    guard session.cleaningUp == nil, session.cleanupUnanswered == nil, session.alert == nil else { return }
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
    // The person moved on while it was read: the alert would ask about a session they no longer look at.
    guard session.sessionChoice == .one(key), shownAlert == nil, !session.alertRunning, !session.recovering else {
      return
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
  /// steps still show. One whose reply never came may still run, so the button waits for
  /// ``checkUnansweredCleanup()`` to see it ended.
  ///
  /// - Parameter key: The session's key.
  func cleanUp(_ key: String) async {
    session.cleaningUp = key
    let sentAt = dependencies.wallClock()
    do {
      let reply = try await service.cleanup(session: key, apply: true)
      session.cleanupReceipts[key] = reply.isEmpty ? nil : reply
      say(ViewerPresenter.cleanupNotice(reply))
    } catch {
      if error.isCleanupUnanswered {
        session.cleanupUnanswered = ViewerUnansweredCleanup(session: key, sentAt: sentAt)
      }
      say(error.isCleanupUnanswered ? ViewerWords.cleanupUnanswered : error.message)
    }
    session.alert = nil
    session.cleaningUp = nil
    panesModel.followSessions(readNow: true)
  }

  /// Reads `cleanup.json` again for a cleanup whose reply never came, each time the sessions' list is read. Once it
  /// shows no step under way, or ``cleanupWaitLimit`` has passed, Clean Up takes a press again, what each step did
  /// stays for Details, and the notice says how it ended; until then the button waits.
  func checkUnansweredCleanup() async {
    guard let unanswered = session.cleanupUnanswered else { return }
    let preview = try? await service.cleanup(session: unanswered.session, apply: false)
    let now = dependencies.wallClock()
    let ended = preview?.isUnderWay == false
    let waitedOut = now.timeIntervalSince(unanswered.sentAt) >= Self.cleanupWaitLimit
    guard session.cleanupUnanswered == unanswered, ended || waitedOut else { return }
    session.cleanupUnanswered = nil
    guard let preview else { return }
    session.cleanupReceipts[unanswered.session] = preview.hasRun ? preview : nil
    if let notice = ViewerPresenter.unansweredCleanupNotice(preview) { say(notice) }
  }
}
