import SwiftUI

/// The app's own icon, from the `appIcon` environment value, or a placeholder tile where none is supplied.
package struct AppIcon: View {
  /// The icon's side, in points.
  let size: CGFloat
  @Environment(\.appIcon) private var icon

  /// Creates the icon.
  ///
  /// - Parameter size: The icon's side, in points.
  package init(size: CGFloat) {
    self.size = size
  }

  /// The icon, or its placeholder.
  package var body: some View {
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
