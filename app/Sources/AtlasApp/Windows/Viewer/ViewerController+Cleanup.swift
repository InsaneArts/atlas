import AtlasService
import AtlasUI
import Foundation

/// Clean Up This Session's Work…: reads what cleaning up the session's work takes, which changes nothing, then asks
/// through ``ask(_:)``, so its Clean Up runs once however often it is chosen. Clean Up cleans up as the person; what
/// each step did stays in the session's tooltip, and the sessions' list is read again. The item waits while either
/// request runs, and while a cleanup whose reply never came may still run, the session's row showing it.
extension ViewerController {
  /// 5 min: how long Clean Up waits on a cleanup whose reply never came while `cleanup.json` still shows a step under
  /// way. Well past the longest cleanup, so a step that a run cut short left started does not hold the item for
  /// good.
  static let cleanupWaitLimit: TimeInterval = 300

  /// Reads what cleaning up `key`'s work takes, then asks before it runs; with nothing to clean up, says so instead.
  /// A choice while a cleanup is read, runs or may still run, or while an alert is up, does nothing; so does a
  /// preview that comes back once another session is chosen or an alert went up meanwhile.
  ///
  /// - Parameter key: The session's key.
  func askCleanup(_ key: String) async {
    guard !session.cleanup.isBusy, session.alert == nil else { return }
    guard !session.alertRunning, !session.recovering else { return say(HelperWords.waitForAction) }
    let chosen = session.chosenSession
    session.cleanup.running = key
    let preview: CleanupReply
    do {
      preview = try await service.cleanup(session: key, apply: false)
    } catch {
      session.cleanup.running = nil
      if !error.isCancelled { say(error.message) }
      return
    }
    session.cleanup.running = nil
    guard !preview.isEmpty else {
      say(ViewerWords.nothingToCleanUp)
      return panesModel.followSessions(readNow: true)
    }
    // The person moved on while it was read: the alert would ask about a session they no longer look at.
    guard session.chosenSession == chosen, shownAlert == nil, !session.alertRunning, !session.recovering else {
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
  /// stays for the session's tooltip, and the sessions' list is read again. One that ran and did not finish says so,
  /// and its steps still show. One whose reply never came may still run, so the item waits for
  /// ``checkUnansweredCleanup()`` to see it ended.
  ///
  /// - Parameter key: The session's key.
  func cleanUp(_ key: String) async {
    session.cleanup.running = key
    let sentAt = dependencies.wallClock()
    do {
      let reply = try await service.cleanup(session: key, apply: true)
      session.cleanup.receipts[key] = reply.isEmpty ? nil : reply
      say(ViewerPresenter.cleanupNotice(reply))
    } catch {
      if error.isCleanupUnanswered {
        session.cleanup.unanswered = ViewerUnansweredCleanup(session: key, sentAt: sentAt)
      }
      say(error.isCleanupUnanswered ? ViewerWords.cleanupUnanswered : error.message)
    }
    session.alert = nil
    session.cleanup.running = nil
    panesModel.followSessions(readNow: true)
  }

  /// What a fresh read of the sessions' list asks of this window's cleanups: what the last ones did is forgotten for
  /// sessions the list no longer names, and a cleanup whose reply never came is read again
  /// (``checkUnansweredCleanup()``), so its item waits until it ended.
  ///
  /// - Parameter list: The list just read.
  func sessionsRead(_ list: HostSessionList) async {
    let named = Set(list.sessions.map(\.key) + list.live.map(\.key))
    session.cleanup.keepReceipts(for: named)
    await checkUnansweredCleanup()
  }

  /// Reads `cleanup.json` again for a cleanup whose reply never came, each time the sessions' list is read. Once it
  /// shows no step under way, or ``cleanupWaitLimit`` has passed, Clean Up can be chosen again, what each step did
  /// stays for the session's tooltip, and the notice says how it ended; until then the item waits.
  func checkUnansweredCleanup() async {
    guard let unanswered = session.cleanup.unanswered else { return }
    let preview = try? await service.cleanup(session: unanswered.session, apply: false)
    let now = dependencies.wallClock()
    let ended = preview?.isUnderWay == false
    let waitedOut = now.timeIntervalSince(unanswered.sentAt) >= Self.cleanupWaitLimit
    guard session.cleanup.unanswered == unanswered, ended || waitedOut else { return }
    session.cleanup.unanswered = nil
    guard let preview else { return }
    session.cleanup.receipts[unanswered.session] = preview.hasRun ? preview : nil
    if let notice = ViewerPresenter.unansweredCleanupNotice(preview) { say(notice) }
  }
}
