/// How a mode reads: a label, the tone of its dot, the mark the menu bar
/// glyph carries, and a one-line reason. Colour is chosen by the UI.
package struct Mode: Equatable, Sendable {
  /// The tone of the mode's dot; the UI picks its colour.
  package enum Tone: Sendable {
    /// All is well.
    case ready
    /// Something is acting in the helper.
    case active
    /// The person is needed.
    case attention
    /// Nothing can happen now.
    case quiet
  }

  /// The mark the menu bar glyph carries.
  package enum Mark: Sendable {
    /// The plain glyph.
    case none
    /// An agent is working.
    case working
    /// A viewer holds control.
    case filled
    /// The person is needed.
    case alert
    /// The helper is not ready.
    case dash
    /// The helper cannot be reached.
    case dimmed
  }

  /// The mode in words, such as "Watching".
  package let label: String
  /// The tone of its dot.
  package let tone: Tone
  /// The mark the menu bar glyph carries.
  package let mark: Mark

  /// Creates a mode.
  ///
  /// - Parameters:
  ///   - label: The mode in words.
  ///   - tone: The tone of its dot.
  ///   - mark: The mark the menu bar glyph carries.
  package init(label: String, tone: Tone, mark: Mark) {
    self.label = label
    self.tone = tone
    self.mark = mark
  }

  /// The helper's mode, as the person reads it.
  ///
  /// - Parameter mode: The helper's mode, as the service names it.
  /// - Returns: Its label, tone and mark.
  package static func of(_ mode: HelperMode) -> Mode {
    switch mode {
    case .observing: Mode(label: "Watching", tone: .ready, mark: .none)
    case .agent: Mode(label: "Agent working", tone: .active, mark: .working)
    case .controlling: Mode(label: "Viewer has control", tone: .active, mark: .filled)
    case .recovering: Mode(label: "Needs you", tone: .attention, mark: .alert)
    case .blocked: Mode(label: "Not ready", tone: .attention, mark: .dash)
    case .unavailable: Mode(label: "Unavailable", tone: .quiet, mark: .dimmed)
    case .loggedOut: Mode(label: "Logged out", tone: .quiet, mark: .dimmed)
    }
  }

  /// The mode as a viewer sees it: the service knows only that some viewer
  /// holds control; a viewer knows whether it is that one.
  ///
  /// - Parameters:
  ///   - overview: The service's overview.
  ///   - holdingControl: Whether this viewer holds control.
  /// - Returns: "You have control" or "Another viewer has control" where that applies, else the helper's mode.
  package static func seen(by overview: ViewerStatus.Overview, holdingControl: Bool) -> Mode {
    let mode = overview.mode
    if mode.isControlled(byThisViewer: holdingControl) {
      return Mode(label: "You have control", tone: .active, mark: .filled)
    }
    if mode == .controlling && !holdingControl {
      return Mode(label: Words.anotherViewer, tone: .active, mark: .filled)
    }
    return of(mode)
  }
}
