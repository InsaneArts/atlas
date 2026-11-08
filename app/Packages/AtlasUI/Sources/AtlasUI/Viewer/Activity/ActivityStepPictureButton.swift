import SwiftUI

/// A step's picture button: `photo` in a small 24 × 20 capsule, tertiary at rest, filled with the symbol primary under
/// the pointer, with a 28-point hit area. It opens the screen just before the step.
struct ActivityStepPictureButton: View {
  /// What pressing it does.
  let action: @MainActor () -> Void

  /// The button.
  var body: some View {
    Button(action: action) {
      Image(systemName: SymbolName.photo.rawValue)
        .font(Typography.symbol(.row))
    }
    .buttonStyle(ActivityStepPictureStyle())
    .help("Show the screen before this step")
    .accessibilityLabel(Text("Show the screen before this step"))
  }
}
