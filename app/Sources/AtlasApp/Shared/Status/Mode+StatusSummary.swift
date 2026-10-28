import AtlasStatus
import AtlasUI

/// The status table, in one place: each row's word colour and mark, and its line in the main window and in the
/// panel. Every word has a line beside it, so a state never stands alone, except where the stage right under it says
/// the same: the main window's Logged out, whose stage says so at length.
extension Mode {
  /// The mode as the main window shows it, beside its word. A paused picture keeps the agent's own line, since the
  /// screen's chip is what says the picture is paused.
  ///
  /// - Parameters:
  ///   - reading: What the window last read.
  ///   - app: The app whose window the screen outlines, which Idle says next; nil for none.
  ///   - reconnecting: Whether answers came before and stopped, so connecting reads as Reconnecting….
  /// - Returns: The word, its colour, its mark and the window's line.
  func windowSummary(
    _ reading: StatusReading,
    showing app: String? = nil,
    reconnecting: Bool = false
  ) -> StatusSummary {
    let line =
      switch kind {
      case .idle: app.map(Words.showingLine(app:)) ?? reading.idleLine
      case .working: reading.workingLine(withAge: false)
      case .control: reading.controlLine
      case .needsYou: reading.needsYouLine
      case .stopped: Words.stoppedLine
      case .inUseElsewhere: Words.inUseElsewhereLine
      case .notReady: PanelWords.notReadyLine(reading.status.overview.reason)
      case .loggedOut: ""
      case .connecting: Self.connectingLine(reconnecting: reconnecting)
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
  /// - Returns: The word, its colour, its mark and the panel's line.
  func panelSummary(_ reading: StatusReading, paused: Bool, reconnecting: Bool = false) -> StatusSummary {
    let line =
      switch kind {
      case .idle: paused ? PanelWords.pausedLine : reading.idleLine
      case .working: reading.workingLine(withAge: true)
      case .control: reading.controlLine
      case .needsYou: reading.needsYouLine
      case .stopped: Words.stoppedLine
      case .notReady: PanelWords.notReadyLine(reading.status.overview.reason)
      case .loggedOut: PanelWords.loggedOutLine
      case .inUseElsewhere: Words.inUseElsewhereLine
      case .connecting: Self.connectingLine(reconnecting: reconnecting)
      }
    return summary(line: line)
  }

  /// The line beside Connecting: the helper starting, the first time, or reconnecting after answers stopped.
  ///
  /// - Parameter reconnecting: Whether answers came before and stopped.
  /// - Returns: The line.
  static func connectingLine(reconnecting: Bool) -> String {
    reconnecting ? Words.reconnectingLine : Words.startingLine
  }

  /// The mode with `line` beside it.
  ///
  /// - Parameter line: The line beside the word.
  /// - Returns: The word, its colour, its mark and the line.
  func summary(line: String) -> StatusSummary {
    StatusSummary(label: word, tone: tone, mark: mark, line: line)
  }

  /// The word's and the mark's colours: blue while working, the link colour in control, orange when the person is
  /// needed, grey otherwise.
  private var tone: StatusTone {
    switch kind {
    case .working: .active
    case .control: .link
    case .needsYou, .stopped: .attention
    case .idle, .notReady, .loggedOut, .connecting, .inUseElsewhere: .quiet
    }
  }

  /// The mark: a breathing dot while working, a square once stopped, a small dot when not ready, else a dot.
  private var mark: StatusMark {
    switch kind {
    case .working: .pulsing
    case .stopped: .square
    case .notReady: .small
    case .idle, .control, .needsYou, .loggedOut, .connecting, .inUseElsewhere: .dot
    }
  }
}
