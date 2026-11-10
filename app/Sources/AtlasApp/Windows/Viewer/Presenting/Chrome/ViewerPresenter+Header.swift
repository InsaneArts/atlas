import AtlasStatus
import AtlasUI
import Foundation

extension ViewerPresenter {
  /// The card's header, who has the helper: "You have control of the helper" while this window holds control; the
  /// helper's state alone while it asks something of the person or cannot be used, such as Needs you, Stopped or Not
  /// ready, in its tone; while the picture is paused, "Paused picture from 10:31", when the newest picture was read,
  /// or "Picture paused" before any was; the session that has the helper and "has the helper" while sessions show;
  /// else the helper's state.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The header.
  static func header(_ snapshot: ViewerSnapshot) -> ViewerHeaderState {
    if snapshot.holdingControl { return ViewerHeaderState(who: ViewerWords.you, what: ViewerWords.haveControl) }
    let mode = snapshot.mode
    let word = ViewerHeaderState(who: mode.summary(line: "").label, tone: mode.summary(line: "").tone)
    switch mode.kind {
    case .needsYou, .stopped, .inUseElsewhere, .notReady, .loggedOut, .connecting: return word
    case .idle, .working, .control: break
    }
    if snapshot.session.paused {
      guard let taken = pictureTaken(snapshot) else {
        return ViewerHeaderState(who: ViewerWords.picturePaused, what: ViewerWords.workGoesOn)
      }
      return ViewerHeaderState(who: ViewerWords.paused, what: ViewerWords.pictureFrom(minute(taken, snapshot)))
    }
    let holder = snapshot.reading.statusReading.holder?.key
    // With one session chosen, a list names it, as its Activity outline leaves out the session's own row.
    if let chosen = snapshot.shownSessions.key, snapshot.session.place != .screen {
      return sessionHeader(chosen, holds: chosen == holder, snapshot)
    }
    guard let holder else { return word }
    return sessionHeader(holder, holds: true, snapshot)
  }

  /// The header naming a session: its title from its project or agent, then its agent while the title is its project,
  /// and whether it has the helper, such as "atlas" and "Claude Code · has the helper".
  ///
  /// - Parameters:
  ///   - key: The session's key.
  ///   - holds: Whether it has the helper.
  ///   - snapshot: What the controller holds.
  /// - Returns: The header.
  static func sessionHeader(_ key: String, holds: Bool, _ snapshot: ViewerSnapshot) -> ViewerHeaderState {
    let (_, agent, project, _) = sessionHead(.session(key), snapshot)
    return ViewerHeaderState(
      who: sessionTitle(agent: agent, project: project),
      what: ViewerWords.dotted([project.isEmpty ? "" : agent, holds ? ViewerWords.hasTheHelper : ""])
    )
  }

  /// When the newest picture was read, on the wall clock: as long before now as the picture is old.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The moment, or nil before the first picture.
  static func pictureTaken(_ snapshot: ViewerSnapshot) -> Date? {
    guard snapshot.reading.picture != nil, let frameAt = snapshot.reading.frameAt else { return nil }
    return snapshot.date.addingTimeInterval(-max(0, snapshot.now - frameAt))
  }

  /// The helper's state as this window sees it, with its line: the mode's word and line, Reconnecting… once a picture
  /// was lost, and while the helper needs the person and sessions show, whose action may not have finished. VoiceOver
  /// says it while it asks something of the person, and the footer says its line while nothing newer is to be said.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The status.
  static func status(_ snapshot: ViewerSnapshot) -> StatusSummary {
    let reading = snapshot.reading.statusReading
    var summary = snapshot.mode.windowSummary(reading, reconnecting: snapshot.reading.lastPicture != nil)
    if snapshot.mode.kind == .needsYou, let line = reading.causerLine { summary.line = line }
    return summary
  }
}
