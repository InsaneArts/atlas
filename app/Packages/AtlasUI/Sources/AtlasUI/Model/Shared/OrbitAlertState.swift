/// An alert in the widget's look: Atlas's icon, a title, what it means, and its buttons, top to bottom.
public struct OrbitAlertState<ID: Hashable & Sendable>: Equatable, Sendable {
  /// The question or the news, such as "Recover the helper?".
  public var title: String
  /// What it means, in a sentence or two.
  public var message: String
  /// The buttons, top to bottom: the action then Cancel, or, in a destructive alert, Cancel then the action.
  public var buttons: [OrbitAlertButton<ID>]
  /// Whether its action runs now, so every button waits.
  public var isBusy: Bool

  /// Creates an alert's state.
  ///
  /// - Parameters:
  ///   - title: The question or the news.
  ///   - message: What it means, in a sentence or two.
  ///   - buttons: The buttons, top to bottom.
  ///   - isBusy: Whether its action runs now, so every button waits.
  public init(title: String, message: String, buttons: [OrbitAlertButton<ID>], isBusy: Bool = false) {
    self.title = title
    self.message = message
    self.buttons = buttons
    self.isBusy = isBusy
  }

  /// Whether Escape needs a key of its own: no button takes it, as in a destructive alert, whose way out takes Return.
  var escapeNeedsKey: Bool {
    !buttons.contains { $0.role == .cancel }
  }

  /// What Escape sends: the way out, if the alert has one and its action isn't running, as its buttons wait then too.
  var escape: ID? {
    guard !isBusy else { return nil }
    return buttons.first { $0.role == .cancel || $0.role == .safeCancel }?
      .id
  }
}
