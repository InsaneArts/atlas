import SwiftUI

/// The selected track tab's thumb: a raised capsule, white under Light and lighter than its track under Dark, with a
/// fine rim and, under Light, a faint shadow, as a segmented control's thumb.
struct TabsThumb: View {
  /// Whether a pointer is holding the control.
  var isPressed = false

  /// The capsule on its rim.
  var body: some View {
    Capsule()
      .fill(Color.fillSelection)
      .overlay {
        Capsule()
          .fill(isPressed ? Color.fillSelectionPressed : .clear)
      }
      .overlay(
        CapsuleEdge()
          .strokeBorder(Color.strokeSelected, lineWidth: Stroke.hairline)
      )
      .elevation(.selection)
  }
}

#if DEBUG
  #Preview("Thumb") {
    TabsThumb()
      .frame(width: IconSize.xxl, height: TabsLayout.trackHeight)
      .padding()
      .sampleSurface()
  }
#endif
