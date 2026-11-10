import SwiftUI

/// How a step's picture button draws: its symbol in a 24 × 20 capsule, filled and primary under the pointer, in a
/// 28-point square hit area.
struct ActivityStepPictureStyle: ButtonStyle {
  /// The button.
  ///
  /// - Parameter configuration: Its symbol and whether it is pressed.
  /// - Returns: The button.
  func makeBody(configuration: Configuration) -> some View {
    ActivityStepPictureBody(configuration: configuration)
  }
}
