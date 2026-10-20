import SwiftUI

extension PrimitiveButtonStyle where Self == MenuRowStyle {
  /// A button set like a menu item.
  ///
  /// - Parameter shortcut: The shortcut written on the right; nil writes none.
  /// - Returns: The style.
  static func menuRow(shortcut: KeyboardShortcut? = nil) -> MenuRowStyle {
    MenuRowStyle(shortcut: shortcut)
  }
}
