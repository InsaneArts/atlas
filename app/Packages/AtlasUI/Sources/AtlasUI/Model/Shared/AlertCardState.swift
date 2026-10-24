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

  /// Whether its command runs or has just worked, so its asked buttons wait.
  public var isBusy: Bool {
    switch phase {
    case .running, .done: true
    case .asking, .refused: false
    }
  }

  /// Whether its command runs now, before it worked or was refused.
  var isRunning: Bool {
    if case .running = phase { return true }
    return false
  }

  /// Whether its command worked and the alert holds before it closes.
  var isDone: Bool {
    if case .done = phase { return true }
    return false
  }

  /// The title shown: the done title once done, such as "The helper is ready", else the question.
  var shownTitle: String {
    if case .done(let title, _, _) = phase { return title }
    return title
  }

  /// The message shown: the done message once done, such as "Mouse and keyboard work again.", else what it means. The
  /// card keeps the asked message's height either way.
  var shownMessage: String {
    if case .done(_, let message, _) = phase { return message }
    return message
  }

  /// Why its command was refused, shown in place of the message; nil until it was.
  var refusal: String? {
    if case .refused(let message) = phase { return message }
    return nil
  }

  /// The word a button shows: always its own, so no button changes its width as its command runs.
  ///
  /// - Parameter button: The button.
  /// - Returns: Its word.
  func label(of button: AlertCardButton<ID>) -> String {
    button.title
  }

  /// What VoiceOver says for a button: the running word, such as "Recovering…", on the button that runs the command
  /// while it runs, else its own word.
  ///
  /// - Parameter button: The button.
  /// - Returns: Its spoken word now.
  func spokenLabel(of button: AlertCardButton<ID>) -> String {
    guard button.runs, case .running(let label) = phase else { return button.title }
    return label
  }

  /// Whether a button holds the work now: the one that runs the command, while it runs.
  ///
  /// - Parameter button: The button.
  /// - Returns: Whether it shows a spinner.
  func isWorking(_ button: AlertCardButton<ID>) -> Bool {
    button.runs && isRunning
  }

  /// The one button a done alert shows in place of the rest: its word after a check, the default, which
  /// Return presses, sending the way out, so the alert closes now rather than when its hold ends. Nil before it is
  /// done, or for an alert with no way out.
  var doneButton: AlertCardButton<ID>? {
    guard case .done(_, _, let word) = phase, let wayOut else { return nil }
    return AlertCardButton(id: wayOut, title: word, role: .action)
  }

  /// Whether Escape needs a key of its own: no button takes it, as in a destructive alert, whose way out takes Return.
  var escapeNeedsKey: Bool {
    !buttons.contains { $0.role == .cancel }
  }

  /// What Escape sends: the way out, if the alert has one and its command isn't running, as its buttons wait then
  /// too. Once done it closes the alert, as Done does.
  var escape: ID? {
    guard !isRunning else { return nil }
    return wayOut
  }

  /// What the way out sends, Not Now's or Cancel's; nil for an alert with none.
  private var wayOut: ID? {
    buttons.first { $0.role == .cancel || $0.role == .safeCancel }?
      .id
  }
}
