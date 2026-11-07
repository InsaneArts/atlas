/// Whether a screenshot shows the whole screen, which the viewer draws filling its 16:10 frame, or a window, which it
/// draws standing inside the frame. The list names no kind, so a screenshot shows the whole screen when it is the
/// largest the list holds and 16:10 itself, so filling the frame cuts nothing.
public enum ScreenshotShape {
  /// 1.6: the frames' shape, width over height.
  static let frame = 1.6
  /// 0.02: how far a screenshot's shape may lie from the frame's and still fill it.
  static let tolerance = 0.02

  /// Whether the screenshot `name` fills its frame.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - listed: The screenshots the service lists, with their sizes.
  /// - Returns: Whether it shows the whole screen; false for one the list does not size.
  public static func fillsFrame(_ name: String, in listed: [Screenshot]) -> Bool {
    guard let shot = listed.first(where: { $0.name == name }), let width = shot.width, let height = shot.height,
      width > 0, height > 0
    else { return false }
    let largest =
      listed.compactMap { shot in shot.width.flatMap { width in shot.height.map { width * $0 } } }
      .max() ?? 0
    return width * height >= largest && abs(Double(width) / Double(height) - frame) <= tolerance
  }
}
