import AtlasService
import Foundation

extension DesktopPoint {
  /// 0.001 of a view point: how far inside the picture's right and bottom edges a clamped point stays. A point
  /// exactly on those edges lies outside the picture, which ends there, and a thousandth of a point is far less than
  /// one of the display's pixels at any size the view draws it.
  static let edgeInset = 0.001

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
  public init?(
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
      x = max(0, min(width - Self.edgeInset, x))
      y = max(0, min(height - Self.edgeInset, y))
    } else if x < 0 || y < 0 || x >= width || y >= height {
      return nil
    }
    self.init(x: x / width * display.width, y: y / height * display.height)
  }

  /// View points per point of the helper's display, where a view of `view`'s size shows the picture letterboxed at
  /// its own shape: how far a move on the view is, as the person's hand moved it.
  ///
  /// Fails when any size is empty or not finite.
  ///
  /// - Parameters:
  ///   - view: The view's size.
  ///   - image: The picture's size, in pixels.
  ///   - display: The helper's display, in points.
  /// - Returns: The view points in one display point.
  public static func scale(in view: CGSize, image: CGSize, display: ViewerStatus.Size) -> Double? {
    let sizes = [view.width, view.height, image.width, image.height, display.width, display.height]
    guard sizes.allSatisfy({ $0.isFinite && $0 > 0 }) else { return nil }
    let shown = image.width * min(view.width / image.width, view.height / image.height)
    return shown / display.width
  }

  /// Where the point lies as a share of the helper's display: 0 at its left or top edge, 1 at its right or bottom,
  /// held to that span, as a window outline is placed over the picture (`ViewerLayout.outlineFrame`, which letterboxes
  /// it as ``init(location:in:image:display:clamp:)`` maps a click).
  ///
  /// Fails when the display is empty or not finite.
  ///
  /// - Parameter display: The helper's display, in points.
  /// - Returns: The share across and down.
  public func fraction(of display: ViewerStatus.Size) -> CGPoint? {
    guard [display.width, display.height].allSatisfy({ $0.isFinite && $0 > 0 }) else { return nil }
    return CGPoint(x: min(1, max(0, x / display.width)), y: min(1, max(0, y / display.height)))
  }
}
