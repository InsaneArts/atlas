import AppKit

/// A menu item that runs a closure, such as a key in the Keys menu.
///
/// `nonisolated`, as NSMenuItem is: the app's main-actor default would make the initializer stubs Swift writes for
/// NSMenuItem's own designated initializers main-actor bound, which an override of a nonisolated one may not be. Only
/// choosing the item runs on the main actor, where AppKit sends its action.
nonisolated final class HandlerMenuItem: NSMenuItem {
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
  @MainActor @objc func fire() { handler() }
}
