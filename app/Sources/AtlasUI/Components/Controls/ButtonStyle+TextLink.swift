import SwiftUI

extension ButtonStyle where Self == TextLinkStyle {
  /// A link's look, for a button among a row's words.
  static var textLink: TextLinkStyle { TextLinkStyle() }

  /// A link's look at a size.
  ///
  /// - Parameter size: How large its words are.
  /// - Returns: The style.
  static func textLink(_ size: TextLinkStyle.Size) -> TextLinkStyle {
    TextLinkStyle(size: size)
  }
}
