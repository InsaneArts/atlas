import Foundation

/// Retains only the latest invoking button's focus and falls back when that button leaves the page.
@MainActor
final class SettingsAlertFocus {
  /// The button whose action most recently opened an alert.
  private var invoker: UUID?
  /// Restores that button's SwiftUI focus.
  private var returnFocus: (() -> Void)?

  /// Remembers the button before its action disables the page.
  ///
  /// - Parameters:
  ///   - id: Its stable view identity.
  ///   - focus: Its focus binding writer.
  func remember(_ id: UUID, focus: @escaping () -> Void) {
    invoker = id
    returnFocus = focus
  }

  /// Removes a button that is no longer available, including a successfully forgotten project.
  ///
  /// - Parameter id: The departing button.
  func forget(_ id: UUID) {
    guard invoker == id else { return }
    invoker = nil
    returnFocus = nil
  }

  /// Restores the button if it survives, otherwise the relevant page heading.
  ///
  /// - Parameter fallback: Focuses the page without scrolling it.
  func restore(or fallback: @escaping () -> Void) {
    DispatchQueue.main
      .async { [self] in
        let focus = returnFocus
        invoker = nil
        returnFocus = nil
        if let focus { focus() } else { fallback() }
      }
  }
}
