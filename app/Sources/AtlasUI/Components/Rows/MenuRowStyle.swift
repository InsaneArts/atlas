import SwiftUI

/// A button set like a menu item: plain text, highlighted under the pointer, with its shortcut on the right. A
/// style, so the row stays a `Button` with its own title, shortcut and disabled state. A primitive style, as it
/// draws a plain button inside, which dims a disabled title as the menu rows always have.
struct MenuRowStyle: PrimitiveButtonStyle {
  /// The shortcut written on the right, such as ⌘Q; nil writes none. The button sets it as its own shortcut too.
  var shortcut: KeyboardShortcut?

  /// Draws the title as a menu item.
  ///
  /// - Parameter configuration: The button's label and its action.
  /// - Returns: The row.
  func makeBody(configuration: Configuration) -> some View {
    MenuRowBody(configuration: configuration, detail: shortcut?.symbols)
  }
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.menu)
  }
#endif
