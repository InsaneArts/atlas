import Foundation

extension DesktopPoint {
  /// Where a point in the view showing the desktop falls on the helper's
  /// display. The view shows the picture letterboxed at its own shape; a
  /// point outside the picture is nowhere, unless `clamp`ed to its edge (a
  /// drag that left the view). `location` is from the view's top left.
  ///
  /// Fails when the point is outside the picture and not clamped, or when any size is empty or not finite.
  ///
  /// - Parameters:
  ///   - location: The point in the view, from its top left.
  ///   - view: The view's size.
  ///   - image: The picture's size, in pixels.
  ///   - display: The helper's display, in points.
  ///   - clamp: Whether a point outside the picture moves to its nearest edge.
  package init?(
    location: CGPoint,
    in view: CGSize,
    image: CGSize,
    display: ViewerStatus.Size,
    clamp: Bool = false
  ) {
    let sizes = [view.width, view.height, image.width, image.height, display.width, display.height]
    guard sizes.allSatisfy({ $0.isFinite && $0 > 0 }) else { return nil }
    let scale = min(view.width / image.width, view.height / image.height)
    let width = image.width * scale
    let height = image.height * scale
    var x = location.x - (view.width - width) / 2
    var y = location.y - (view.height - height) / 2
    if clamp {
      x = max(0, min(width - 0.001, x))
      y = max(0, min(height - 0.001, y))
    } else if x < 0 || y < 0 || x >= width || y >= height {
      return nil
    }
    self.init(x: x / width * display.width, y: y / height * display.height)
  }
}
