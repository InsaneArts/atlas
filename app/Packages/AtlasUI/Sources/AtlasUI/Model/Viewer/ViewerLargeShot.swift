import CoreGraphics

/// One screenshot shown large over the whole window: the picture, its time and size, and what its buttons can do.
public struct ViewerLargeShot: Equatable, Sendable {
  /// The screenshot's file name.
  public var id: String
  /// The picture: as taken once read, its thumbnail before then, nil before either.
  public var picture: DesktopPicture?
  /// The picture's shape, which its frame keeps.
  public var aspect: CGSize
  /// Its time and size, such as "10:31:44 · 2560 × 1440".
  public var caption: String
  /// Whether Previous goes anywhere.
  public var canGoBack: Bool
  /// Whether Next goes anywhere.
  public var canGoForward: Bool
  /// Whether Show in Finder can find the file.
  public var canShowInFinder: Bool

  /// Creates the large view's state.
  ///
  /// - Parameters:
  ///   - id: The screenshot's file name.
  ///   - picture: The picture.
  ///   - aspect: The picture's shape.
  ///   - caption: Its time and size.
  ///   - canGoBack: Whether Previous goes anywhere.
  ///   - canGoForward: Whether Next goes anywhere.
  ///   - canShowInFinder: Whether Show in Finder can find the file.
  public init(
    id: String,
    picture: DesktopPicture?,
    aspect: CGSize,
    caption: String,
    canGoBack: Bool,
    canGoForward: Bool,
    canShowInFinder: Bool
  ) {
    self.id = id
    self.picture = picture
    self.aspect = aspect
    self.caption = caption
    self.canGoBack = canGoBack
    self.canGoForward = canGoForward
    self.canShowInFinder = canShowInFinder
  }
}
