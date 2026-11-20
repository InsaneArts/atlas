import AtlasStatus
import AtlasUI

/// The status table, in one place: each row's line in the main window and in the panel, beside its word in its colour
/// and mark (`Mode.Kind+Look`). Every word has a line beside it, so a state never stands alone, except where the stage right under it says
/// the same: the main window's Logged out, whose stage says so at length.
extension Mode {
  /// The mode as the main window shows it, beside its word. A paused picture keeps the agent's own line, since the
  /// screen's chip is what says the picture is paused.
  ///
  /// - Parameters:
  ///   - reading: What the window last read.
  ///   - reconnecting: Whether answers came before and stopped, so connecting reads as Reconnecting….
  ///   - noun: How the desktop shown is named; the helper by default.
  /// - Returns: The word, its colour, its mark and the window's line.
  func windowSummary(_ reading: StatusReading, reconnecting: Bool = false, noun: DesktopNoun = .helper) -> StatusSummary
  {
    let line =
      switch kind {
      case .idle: reading.idleLine
      case .working: reading.workingLine(withAge: false)
      case .control: reading.controlLine
      case .needsYou: reading.needsYouLine
      case .stopped: Words.stoppedLine
      case .inUseElsewhere: Words.inUseElsewhereLine
      case .notReady: PanelWords.notReadyLine(reading.status.overview.reason, noun: noun)
      case .loggedOut: ""
      case .connecting: Self.connectingLine(reconnecting: reconnecting, noun: noun)
      }
    return summary(line: line)
  }

  /// The mode as the panel shows it, its line under its word.
  ///
  /// - Parameters:
  ///   - reading: What the panel last read.
  ///   - paused: Whether the panel's picture is paused, which Idle says in place of what was done last, as the
  ///     panel's picture carries no word of its own for it.
  ///   - reconnecting: Whether answers came before and stopped, so connecting reads as Reconnecting….
  ///   - noun: How the desktop shown is named; the helper by default.
  /// - Returns: The word, its colour, its mark and the panel's line.
  func panelSummary(
    _ reading: StatusReading,
    paused: Bool,
    reconnecting: Bool = false,
    noun: DesktopNoun = .helper
  ) -> StatusSummary {
    let line =
      switch kind {
      case .idle: paused ? noun.pausedLine : reading.idleLine
      case .working: reading.workingLine(withAge: true)
      case .control: reading.controlLine
      case .needsYou: reading.needsYouLine
      case .stopped: Words.stoppedLine
      case .notReady: PanelWords.notReadyLine(reading.status.overview.reason, noun: noun)
      case .loggedOut: PanelWords.loggedOutLine
      case .inUseElsewhere: Words.inUseElsewhereLine
      case .connecting: Self.connectingLine(reconnecting: reconnecting, noun: noun)
      }
    return summary(line: line)
  }

  /// The line beside Connecting: the helper starting, the first time, or reconnecting after answers stopped.
  ///
  /// - Parameters:
  ///   - reconnecting: Whether answers came before and stopped.
  ///   - noun: How the desktop shown is named; the helper by default.
  /// - Returns: The line.
  static func connectingLine(reconnecting: Bool, noun: DesktopNoun = .helper) -> String {
    reconnecting ? Words.reconnectingLine : noun.startingLine
  }

  /// The mode with `line` beside it.
  ///
  /// - Parameter line: The line beside the word.
  /// - Returns: The word, its colour, its mark and the line.
  func summary(line: String) -> StatusSummary {
    StatusSummary(label: word, tone: kind.tone, mark: kind.mark, line: line)
  }
}
