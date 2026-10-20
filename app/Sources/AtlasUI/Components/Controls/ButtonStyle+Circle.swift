import SwiftUI

extension ButtonStyle where Self == CircleButtonStyle {
  /// A round glass button holding a symbol.
  ///
  /// - Parameters:
  ///   - size: How large the circle is; regular by default.
  ///   - isOn: Whether what it toggles is on, which a brighter fill shows.
  /// - Returns: The style.
  static func circle(_ size: CircleButtonStyle.Size = .regular, isOn: Bool = false) -> CircleButtonStyle {
    CircleButtonStyle(size: size, isOn: isOn)
  }
}
