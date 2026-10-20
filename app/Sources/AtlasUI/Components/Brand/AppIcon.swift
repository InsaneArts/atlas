import SwiftUI

/// The app's own icon, from the `appIcon` environment value, or a placeholder tile where none is supplied.
struct AppIcon: View {
  /// The icon's side, in points.
  let size: CGFloat
  @Environment(\.appIcon) private var icon

  var body: some View {
    if let icon {
      icon
        .resizable()
        .interpolation(.high)
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    } else {
      RoundedRectangle(cornerRadius: Radius.large)
        .fill(Color.fillControl)
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
  }
}

#if DEBUG
  #Preview("Placeholder") {
    AppIcon(size: IconSize.hero)
      .sampleSurface()
  }
#endif
