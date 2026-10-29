import AtlasStatus
import AtlasUI
import Foundation

extension PanelPresenter {
  /// Sessions' rows while sessions show and the panel shows activity at all, at most ``PanelState/mostSessions``, in
  /// the service's order, each with what VoiceOver reads for it; else none, and Recent shows as before.
  ///
  /// - Parameters:
  ///   - reading: The status, read.
  ///   - mode: The helper's mode.
  ///   - now: The time the snapshot was taken, which an ended session's age counts to.
  /// - Returns: The rows.
  static func sessions(_ reading: StatusReading, mode: Mode, now: Date) -> [PanelSessionEntry] {
    guard reading.showsSessions, !hidesRecent(mode) else { return [] }
    return reading.liveSessions
      .prefix(PanelState.mostSessions)
      .map { session in
        let line = Words.sessionLine(reading.line(of: session), now: now)
        let state = Words.sessionState(session.state)
        return PanelSessionEntry(
          id: session.key,
          icon: session.host.icon,
          name: session.name,
          line: line,
          isUrgent: session.state == .needsYou,
          state: state,
          tone: session.state.tone,
          mark: session.state.mark,
          label: PanelWords.sessionLabel(name: session.name, state: state, line: line)
        )
      }
  }

  /// The row under the rows that opens every session, while there are more live, or listed past them, than the rows
  /// show; nil otherwise.
  ///
  /// - Parameters:
  ///   - reading: The status, read.
  ///   - rows: The rows the panel shows.
  /// - Returns: "+2 more", or nil.
  static func moreSessions(_ reading: StatusReading, rows: [PanelSessionEntry]) -> String? {
    guard !rows.isEmpty else { return nil }
    let more = reading.sessionsPast(rows.count)
    return more > 0 ? PanelWords.moreSessions(more) : nil
  }

  /// The line under the heading while sessions show: whose action may not have finished while the helper needs the
  /// person, else who has the helper while one does; nil keeps the status's own line.
  ///
  /// - Parameters:
  ///   - reading: The status, read.
  ///   - mode: The helper's mode.
  /// - Returns: The line, or nil.
  static func sessionLine(_ reading: StatusReading, mode: Mode) -> String? {
    guard reading.showsSessions, !hidesRecent(mode) else { return nil }
    if mode.kind == .needsYou { return reading.causerLine }
    return reading.holder.map { Words.holderLine($0.name) }
  }
}
