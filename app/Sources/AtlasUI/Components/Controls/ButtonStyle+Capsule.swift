import SwiftUI

extension ButtonStyle where Self == CapsuleButtonStyle {
  /// Atlas's capsule button.
  ///
  /// - Parameters:
  ///   - look: How the capsule is painted; primary by default.
  ///   - size: How large it is; regular by default.
  ///   - fill: Whether it takes the whole width offered.
  /// - Returns: The style.
  static func capsule(
    _ look: CapsuleButtonStyle.Look = .primary,
    size: CapsuleButtonStyle.Size = .regular,
    fill: Bool = false
  ) -> CapsuleButtonStyle {
    CapsuleButtonStyle(look: look, size: size, fill: fill)
  }
}
