import CoreGraphics

extension AskCardBody {
  /// 3: how many times the element's size the part spans, so the element reads with what is around it.
  static let room: CGFloat = 3
  /// The whole picture, as shares of it: what an element must meet to be ringed.
  static let whole = CGRect(x: 0, y: 0, width: 1, height: 1)

  /// The part of a picture a question's card shows: the element and the screen around it, cut to the well's shape
  /// (``AskCardState/pictureAspect``) from the same pixels, with the element ringed.
  ///
  /// - Parameters:
  ///   - picture: The picture of the helper's whole screen.
  ///   - element: The element, as shares of the picture from its top-leading corner.
  /// - Returns: The body, or nil when the element lies outside the picture.
  public static func ringed(_ picture: DesktopPicture, element: CGRect) -> AskCardBody? {
    let aspect = AskCardState.pictureAspect
    guard let ratio = picture.aspectRatio, ratio > 0, element.intersects(whole)
    else { return nil }
    // A share across is `ratio` times a share down, in pixels, so a part `width` across is `width * ratio / aspect`
    // down at the well's shape.
    var width = max(element.width * room, element.height * room * aspect / ratio)
    var height = width * ratio / aspect
    if width > 1 {
      width = 1
      height = ratio / aspect
    }
    if height > 1 {
      height = 1
      width = aspect / ratio
    }
    let share = CGRect(
      x: min(max(element.midX - width / 2, .zero), 1 - width),
      y: min(max(element.midY - height / 2, .zero), 1 - height),
      width: width,
      height: height
    )
    guard let part = picture.cropped(to: share, id: "\(picture.id)-part") else { return nil }
    let ring = CGRect(
      x: (element.minX - share.minX) / width,
      y: (element.minY - share.minY) / height,
      width: element.width / width,
      height: element.height / height
    )
    return .picture(part, ring: ring)
  }
}
