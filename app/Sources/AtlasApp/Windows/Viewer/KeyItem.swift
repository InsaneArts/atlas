import AppKit

/// A menu item that runs a closure (NSMenuItem itself is not main-actor-bound).
@MainActor
final class KeyItem: NSMenuItem {
  /// What choosing the item runs.
  private let handler: @MainActor () -> Void

  /// Creates the item.
  ///
  /// - Parameters:
  ///   - title: The item's title.
  ///   - handler: What choosing it runs.
  init(title: String, handler: @escaping @MainActor () -> Void) {
    self.handler = handler
    super.init(title: title, action: #selector(fire), keyEquivalent: "")
    target = self
  }

  /// Not used: items are made in code.
  ///
  /// - Parameter coder: Unused.
  required init(coder: NSCoder) { fatalError("not used") }

  /// Runs the handler.
  @objc func fire() { handler() }
}
