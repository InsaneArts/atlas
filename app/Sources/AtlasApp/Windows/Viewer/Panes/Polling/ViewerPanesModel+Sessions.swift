import AtlasService
import Foundation

extension ViewerPanesModel {
  /// Discovers ended sessions whenever the window is shown. Status readings refresh that history at most every
  /// ``sessionsEvery``; only multiple live sessions or a chosen session need a separate polling loop.
  /// A service that does not offer the list is asked again with a status reading once
  /// ``ViewerSessionsFacts/askAgainAfter`` has passed, as it may have been updated meanwhile.
  ///
  /// - Parameter readNow: Whether to read at once even while a reading loop runs, as when a session is chosen.
  func followSessions(readNow: Bool = false) {
    guard let viewer, viewer.shown else {
      sessionsReadAt = nil
      return stopSessions()
    }
    let now = uptime()
    guard panes.sessions.asks(at: now)
    else { return stopSessions() }
    let polls = viewer.reading.statusReading.showsSessions || viewer.session.chosenSession != .all
    if work.sessions != nil {
      if !readNow, !work.sessionsPolling || polls {
        // An initial history request can become a live poll without asking twice before its first reply.
        work.sessionsPolling = polls
        return
      }
      stopSessions()
    }
    if !polls, !readNow, let readAt = sessionsReadAt,
      Duration.seconds(now - readAt) < Self.sessionsEvery
    {
      return
    }
    stopSessions()
    work.sessionsPolling = polls
    sessionsReadAt = now
    let number = work.sessionsGeneration
    work.sessions = Task { [weak self, clock] in
      while !Task.isCancelled {
        guard self?.work.sessionsGeneration == number else { return }
        await self?.readSessions(number)
        guard self?.work.sessionsGeneration == number, !Task.isCancelled else { return }
        guard self?.work.sessionsPolling == true else {
          self?.work.sessions = nil
          return
        }
        // A cancelled pause ends at once, and the loop's condition ends the loop.
        try? await clock.sleep(for: Self.sessionsEvery)
      }
    }
  }

  /// Stops reading the host sessions' list: its loop ends and a late reply is dropped.
  func stopSessions() {
    work.sessionsGeneration += 1
    work.sessions?.cancel()
    work.sessions = nil
  }

  /// Reads the host sessions' list. A reply overtaken by a newer start or stop has no effect; a service that does not
  /// offer it is left until ``ViewerSessionsFacts/askAgainAfter`` has passed; another failure is kept, and a cancelled
  /// request says nothing. Once read, the window is told, so a cleanup whose reply never came is read again with it.
  ///
  /// - Parameter number: The reading's generation.
  func readSessions(_ number: Int) async {
    sessionsReadAt = uptime()
    do {
      let list = try await service.sessions()
      guard number == work.sessionsGeneration else { return }
      panes.sessions.list = list
      panes.sessions.failure = nil
      panes.sessions.notOfferedAt = nil
      panes.sessions.notOfferedUptime = nil
      await onSessionsRead(list)
    } catch {
      guard number == work.sessionsGeneration, !error.isCancelled else { return }
      guard !error.isNotOffered else {
        panes.sessions.notOfferedAt = wallClock()
        panes.sessions.notOfferedUptime = uptime()
        panes.sessions.list = nil
        return stopSessions()
      }
      panes.sessions.failure = error.message
    }
  }

}
