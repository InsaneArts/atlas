import SwiftUI

extension ButtonStyle where Self == CapsuleButtonStyle {
  /// Atlas's capsule button.
  ///
  /// - Parameters:
  ///   - look: How the capsule is painted; primary by default.
  ///   - size: How large it is; regular by default.
  ///   - fill: Whether it takes the whole width offered.
  ///   - outline: The shape around the label; a capsule by default, a circle for a symbol alone.
  ///   - isOn: Whether what it toggles is on, which a secondary button shows with the held fill.
  ///   - isWorking: Whether its action runs now, so it stays whole while it cannot be pressed.
  /// - Returns: The style.
  static func capsule(
    _ look: CapsuleButtonStyle.Look = .primary,
    size: CapsuleButtonStyle.Size = .regular,
    fill: Bool = false,
    outline: CapsuleButtonStyle.Outline = .pill,
    isOn: Bool = false,
    isWorking: Bool = false
  ) -> CapsuleButtonStyle {
    CapsuleButtonStyle(
      look: look,
      size: size,
      fill: fill,
      outline: outline,
      isOn: isOn,
      isWorking: isWorking
    )
  }
}
