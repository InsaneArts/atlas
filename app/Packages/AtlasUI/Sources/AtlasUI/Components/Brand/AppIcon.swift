import SwiftUI

/// The app's own icon, from the `appIcon` environment value, or a placeholder tile where none is supplied.
/// The icon file is one square canvas whose visible tile is 824 of its 1024 points, with Apple's shape,
/// shadow and transparent margin around it; it is drawn whole in one square frame, never re-masked or
/// cropped.
public struct AppIcon: View {
  /// The square canvas's side, in points: the whole icon file, margin included.
  let size: CGFloat
  @Environment(\.appIcon) private var icon

  /// Creates the icon by the tile it shows: the canvas is drawn larger by the margin around the tile.
  ///
  /// - Parameter tile: The visible tile's side, in points, such as a window layout's welcome or About tile.
  init(tile: CGFloat) {
    self.size = Self.canvas(tile: tile).width
  }

  /// The icon, or its placeholder.
  public var body: some View {
    if let icon {
      icon
        .resizable()
        .interpolation(.high)
        .scaledToFit()
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    } else {
      RoundedRectangle(cornerRadius: Radius.large)
        .fill(Color.fillControl)
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
  }

  /// The square canvas that shows a tile of a given side: the tile times 1024 over 824, as `AppIcon.icns` draws its
  /// tile in 824 of its 1024 points, the same both ways.
  ///
  /// - Parameter tile: The visible tile's side, in points.
  /// - Returns: The canvas's size, square.
  nonisolated static func canvas(tile: CGFloat) -> CGSize {
    let side = tile * Layout.canvasRatio
    return CGSize(width: side, height: side)
  }
}

/// The icon file's own measures.
private enum Layout {
  /// 1024 / 824: the canvas against its visible tile, from `app/Resources/AppIcon.icns`, whose 1024-point canvas
  /// holds an 824-point tile.
  static let canvasRatio: CGFloat = 1024 / 824
}

#if DEBUG
  #Preview("Placeholder") {
    AppIcon(tile: IconSize.xxl)
      .sampleSurface()
  }

  #Preview("Tiles") {
    BrandSamples(.appIcon)
  }
#endif
