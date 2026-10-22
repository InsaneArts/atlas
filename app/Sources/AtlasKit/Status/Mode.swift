/// What the helper is doing, as the person reads it: one row of the status table, and its word. The service names
/// the states every agent app shows (``OverviewState``); the app adds what only it can know: a helper logged out,
/// a window still connecting, and control held by another window. How each looks is the UI's.
package struct Mode: Equatable, Sendable {
  /// The status table's rows.
  package enum Kind: Equatable, Sendable {
    /// Nothing is happening in the helper.
    case idle
    /// An agent is acting in the helper.
    case working
    /// The person holds control, here or, for the panel, in Atlas's window.
    case control
    /// An action may not have finished, or the helper's input stopped: the person or recovery settles it.
    case needsYou
    /// The person stopped agents.
    case stopped
    /// Agents can't use the helper yet.
    case notReady
    /// The helper's account is logged out.
    case loggedOut
    /// No answer has come yet.
    case connecting
    /// Another window holds control.
    case inUseElsewhere

    /// The row's own word, when the service gives none.
    var word: String {
      switch self {
      case .idle: "Idle"
      case .working: "Working"
      case .control: "You have control"
      case .needsYou: "Needs you"
      case .stopped: "Stopped"
      case .notReady: "Not ready"
      case .loggedOut: "Logged out"
      case .connecting: "Connecting"
      case .inUseElsewhere: Words.inUseElsewhere
      }
    }
  }

  /// The row.
  package let kind: Kind
  /// Its word, such as "Working": the service's own, else the row's.
  package let label: String

  /// Creates a mode.
  ///
  /// - Parameters:
  ///   - kind: The row.
  ///   - label: Its word; nil or empty takes the row's own.
  package init(_ kind: Kind, label: String? = nil) {
    self.kind = kind
    self.label = label.flatMap { $0.isEmpty ? nil : $0 } ?? kind.word
  }

  /// A window before its first answer.
  package static let connecting = Mode(.connecting)

  /// The helper's state as the person reads it: the service's state and word, logged out where the service says so,
  /// and a state this app does not know as not ready.
  ///
  /// - Parameter overview: The service's overview.
  /// - Returns: The mode.
  package static func of(_ overview: ViewerStatus.Overview) -> Mode {
    // The service files a logged-out helper under not ready; the app tells it apart, since the way back differs.
    if overview.mode == .loggedOut { return Mode(.loggedOut) }
    return switch overview.state {
    case .working: Mode(.working, label: overview.label)
    case .idle: Mode(.idle, label: overview.label)
    case .control: Mode(.control, label: overview.label)
    case .needsYou: Mode(.needsYou, label: overview.label)
    case .stopped: Mode(.stopped, label: overview.label)
    case .notReady: Mode(.notReady, label: overview.label)
    case .unknown: Mode(.notReady)
    }
  }

  /// The mode as a viewer sees it: the service knows only that some viewer holds control; a viewer knows whether it
  /// is that one.
  ///
  /// - Parameters:
  ///   - overview: The service's overview.
  ///   - holdingControl: Whether this viewer holds control.
  /// - Returns: You have control or In use elsewhere where that applies, else the helper's mode.
  package static func seen(by overview: ViewerStatus.Overview, holdingControl: Bool) -> Mode {
    if overview.mode.isControlled(byThisViewer: holdingControl) { return Mode(.control) }
    if overview.state == .control { return Mode(.inUseElsewhere) }
    return of(overview)
  }
}
