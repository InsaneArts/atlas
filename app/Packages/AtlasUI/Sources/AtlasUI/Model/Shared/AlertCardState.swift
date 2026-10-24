/// An alert: Atlas's icon, a title, what it means, its buttons, and where its command stands.
public struct AlertCardState<ID: Hashable & Sendable>: Equatable, Sendable {
  /// The question or the news, such as "Recover the helper?".
  public var title: String
  /// What it means, in a sentence or two.
  public var message: String
  /// The buttons, the default first: the action then Not Now, or, in a destructive alert, Cancel then the action.
  /// A stacked alert shows them in this order, top to bottom; side by side, the default goes on the right.
  public var buttons: [AlertCardButton<ID>]
  /// Where its command stands: asked, running, refused or done.
  public var phase: AlertCardPhase

  /// Creates an alert's state.
  ///
  /// - Parameters:
  ///   - title: The question or the news.
  ///   - message: What it means, in a sentence or two.
  ///   - buttons: The buttons, the default first.
  ///   - phase: Where its command stands; asked by default.
  public init(title: String, message: String, buttons: [AlertCardButton<ID>], phase: AlertCardPhase = .asking) {
    self.title = title
    self.message = message
    self.buttons = buttons
    self.phase = phase
  }

  /// Whether its command runs or has just worked, so every button waits and nothing dismisses it.
  public var isBusy: Bool {
    switch phase {
    case .running, .done: true
    case .asking, .refused: false
    }
  }

  /// The title shown: "Recovered" once done, else the question.
  var shownTitle: String {
    if case .done(let title) = phase { return title }
    return title
  }

  /// The message shown: none once done, so it never contradicts Recovered; else what it means.
  var shownMessage: String? {
    if case .done = phase { return nil }
    return message
  }

  /// Why its command was refused, shown in place of the message; nil until it was.
  var refusal: String? {
    if case .refused(let message) = phase { return message }
    return nil
  }

  /// The word a button shows: the running word, or the done title, on the button that runs the command, else its own.
  ///
  /// - Parameter button: The button.
  /// - Returns: Its word now.
  func label(of button: AlertCardButton<ID>) -> String {
    guard button.runs else { return button.title }
    switch phase {
    case .running(let label): return label
    case .done(let title): return title
    case .asking, .refused: return button.title
    }
  }

  /// Whether a button holds the work now: the one that runs the command, while it runs or has just worked.
  ///
  /// - Parameter button: The button.
  /// - Returns: Whether it shows a spinner or a check.
  func isWorking(_ button: AlertCardButton<ID>) -> Bool {
    button.runs && isBusy
  }

  /// Whether Escape needs a key of its own: no button takes it, as in a destructive alert, whose way out takes Return.
  var escapeNeedsKey: Bool {
    !buttons.contains { $0.role == .cancel }
  }

  /// What Escape sends: the way out, if the alert has one and its command isn't running, as its buttons wait then
  /// too.
  var escape: ID? {
    guard !isBusy else { return nil }
    return buttons.first { $0.role == .cancel || $0.role == .safeCancel }?
      .id
  }
}
