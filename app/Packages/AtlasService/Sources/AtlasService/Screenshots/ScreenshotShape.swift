import CoreGraphics

/// Whether a screenshot shows the helper's whole screen, which the viewer draws filling its 16:10 frame, or a window,
/// which it draws standing inside the frame. A capture of the whole screen has the display's own shape and at least its
/// size in points, a window less; it fills the frame only on a display close enough to 16:10 that filling cuts next to
/// nothing.
public enum ScreenshotShape {
  /// 1.6: the frames' shape, width over height.
  static let frame = 1.6
  /// 1%: how far a screenshot's shape may lie from the display's and still be the whole screen.
  static let sameShape = 0.01
  /// 5%: how far the display's shape may lie from the frame's for a whole-screen capture to fill it.
  static let closeToFrame = 0.05

  /// Whether a screenshot fills its frame.
  ///
  /// - Parameters:
  ///   - shot: The screenshot, as the service lists it.
  ///   - display: The helper's display, in points; nil when not known.
  /// - Returns: Whether it shows the whole screen on a display shaped near enough to the frame; false for one the
  ///   list does not size, or while the display is not known.
  public static func fillsFrame(_ shot: Screenshot?, display: CGSize?) -> Bool {
    guard let width = shot?.width, let height = shot?.height, width > 0, height > 0, let display,
      display.width > 0, display.height > 0
    else { return false }
    let screen = display.width / display.height
    let shape = Double(width) / Double(height)
    let whole = Double(width) >= display.width * (1 - sameShape)
    return whole && abs(shape / screen - 1) <= sameShape && abs(screen / frame - 1) <= closeToFrame
  }
}
