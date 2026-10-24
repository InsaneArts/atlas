import SwiftUI

/// Keeps cadence cards unscaled and applies the disabled token exactly once.
struct PictureChoiceStyle: ButtonStyle {
  /// Includes the owning page and alert's availability.
  @Environment(\.isEnabled) private var enabled

  /// Draws the card without the native plain style's additional disabled treatment.
  ///
  /// - Parameter configuration: The card's label and press state.
  /// - Returns: The card at its intended opacity.
  func makeBody(configuration: Configuration) -> some View {
    configuration.label
      .opacity(enabled ? 1 : Opacity.disabled)
  }
}
