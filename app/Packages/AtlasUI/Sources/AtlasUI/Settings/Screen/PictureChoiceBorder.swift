import SwiftUI

/// Selection stays inside the card; keyboard focus adds a halo outside it.
struct PictureChoiceBorder: View {
  /// Whether the card holds the current preference.
  let selected: Bool
  /// Whether the card owns keyboard focus.
  let focused: Bool

  /// The selection edge and the independent keyboard halo.
  var body: some View {
    RoundedRectangle(cornerRadius: Radius.small)
      .strokeBorder(selected ? Color.strokeFocus : Color.clear, lineWidth: Stroke.regular)
      .overlay {
        RoundedRectangle(cornerRadius: Radius.small)
          .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
          .background(
            RoundedRectangle(cornerRadius: Radius.small)
              .stroke(Color.accentRing, lineWidth: SettingsLayout.focusHalo)
          )
          .opacity(focused ? 1 : 0)
      }
      .allowsHitTesting(false)
  }
}
