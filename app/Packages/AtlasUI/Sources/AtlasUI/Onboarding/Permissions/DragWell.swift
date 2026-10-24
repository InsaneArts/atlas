import SwiftUI

/// Atlas's icon in the permission panel, on a quiet well the width of the panel's column, in a group's fill and edge
/// (v3 D6), so it reads as the one thing to pick up. The icon is laid out by its 64-point tile.
struct DragWell<Icon: View>: View {
  /// The icon, which the app makes draggable.
  let icon: Icon

  /// The icon, centred in its well.
  var body: some View {
    icon
      .frame(width: Self.canvas.width, height: Self.canvas.height)
      .padding(.vertical, Spacing.s)
      .frame(maxWidth: .infinity)
      .background(
        RoundedRectangle(cornerRadius: Radius.large)
          .fill(Color.fillGroup)
      )
      .overlay(
        RoundedRectangle(cornerRadius: Radius.large)
          .strokeBorder(Color.separator, lineWidth: Stroke.hairline)
      )
  }

  /// The icon's canvas: its tile and the margin around it.
  static var canvas: CGSize {
    AppIcon.canvas(tile: OnboardingLayout.dragTile)
  }
}

#if DEBUG
  #Preview("Drag well") {
    DragWell(icon: AppIcon(tile: OnboardingLayout.dragTile))
      .frame(width: OnboardingLayout.sideColumn)
      .padding(Spacing.xl)
      .sampleSurface()
  }
#endif
