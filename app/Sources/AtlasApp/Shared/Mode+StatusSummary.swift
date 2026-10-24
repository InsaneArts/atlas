import AtlasStatus
import AtlasUI

/// The status table, in one place: each row's word colour and mark, and its line in the main window and in the
/// panel (docs/atlas-app-build-plan.md, "Status").
extension Mode {
  /// The mode as the main window shows it, beside its word.
  ///
  /// - Parameters:
  ///   - reading: What the window last read.
  ///   - paused: Whether the picture is paused, which Idle says in place of what was done last.
  ///   - app: The app whose window the screen outlines, which Idle says next; nil for none.
  /// - Returns: The word, its colour, its mark and the window's line.
  func windowSummary(_ reading: StatusReading, paused: Bool, showing app: String? = nil) -> StatusSummary {
    let line =
      switch kind {
      case .idle: paused ? Words.pausedLine : app.map(Words.showingLine(app:)) ?? reading.idleLine
      case .working: reading.workingLine(withAge: false)
      case .control: Words.controlLine
      case .needsYou: reading.needsYouLine
      case .stopped: Words.stoppedLine
      case .inUseElsewhere: Words.inUseElsewhereLine
      case .notReady, .loggedOut, .connecting: ""
      }
    return summary(line: line)
  }

  /// The mode as the panel shows it, beside its word.
  ///
  /// - Parameter reading: What the panel last read.
  /// - Returns: The word, its colour, its mark and the panel's line.
  func panelSummary(_ reading: StatusReading) -> StatusSummary {
    let line =
      switch kind {
      case .idle: reading.idleLine
      case .working: reading.workingLine(withAge: true)
      case .control: PanelWords.controlLine
      case .needsYou: reading.needsYouLine
      case .stopped: Words.stoppedLine
      case .notReady: PanelWords.notReadyLine(reading.status.overview.reason)
      case .loggedOut: PanelWords.loggedOutLine
      case .inUseElsewhere: Words.inUseElsewhereLine
      case .connecting: ""
      }
    return summary(line: line)
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
