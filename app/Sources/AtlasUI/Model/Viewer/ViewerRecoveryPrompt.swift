/// The recovery alert over the desktop: what it says, and whether recovery is running.
package struct ViewerRecoveryPrompt: Equatable, Sendable {
  /// The alert's title.
  package var title: String
  /// Why recovery is needed.
  package var reason: String
  /// Whether recovery is running, which holds its buttons.
  package var isBusy: Bool
  /// The Recover button's title: Recovering… while recovery runs.
  package var recoverTitle: String

  /// Creates the recovery alert's words.
  ///
  /// - Parameters:
  ///   - title: The alert's title.
  ///   - reason: Why recovery is needed.
  ///   - isBusy: Whether recovery is running, which holds its buttons.
  ///   - recoverTitle: The Recover button's title: Recovering… while recovery runs.
  package init(title: String, reason: String, isBusy: Bool, recoverTitle: String) {
    self.title = title
    self.reason = reason
    self.isBusy = isBusy
    self.recoverTitle = recoverTitle
  }
}
