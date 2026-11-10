import CoreGraphics

extension DesktopPicture {
  /// A part of the picture, such as one window of the helper's screen: cut from the same pixels, so it costs no copy
  /// and no capture, and named apart from the whole.
  ///
  /// - Parameters:
  ///   - share: The part, as shares of the picture from its top-leading corner.
  ///   - id: The part's name, such as the frame's id and the window's number.
  /// - Returns: The part, or nil when it lies outside the picture or is under a pixel across.
  public func cropped(to share: CGRect, id: String) -> DesktopPicture? {
    let width = CGFloat(image.width)
    let height = CGFloat(image.height)
    let pixels = CGRect(
      x: share.minX * width,
      y: share.minY * height,
      width: share.width * width,
      height: share.height * height
    )
    .integral
    .intersection(CGRect(x: 0, y: 0, width: width, height: height))
    guard !pixels.isNull, pixels.width >= 1, pixels.height >= 1, let part = image.cropping(to: pixels) else {
      return nil
    }
    return DesktopPicture(id: id, image: part)
  }
}
