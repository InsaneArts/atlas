import CoreGraphics

/// What the live picture's slot draws (v3 V17): the newest picture, with the colour its look keeps and the corners it
/// is cut to. The app draws it through a layer, previews and renders through an image (``StillPicture``); the black
/// that darkens a look is laid over either by the screen.
public struct ViewerLivePicture: Equatable, Sendable {
  /// The picture.
  public var picture: DesktopPicture
  /// How much colour it keeps, 1 for all of it.
  public var saturation: Double
  /// Its corners' radius.
  public var corner: CGFloat

  /// Creates what the slot draws.
  ///
  /// - Parameters:
  ///   - picture: The picture.
  ///   - saturation: How much colour it keeps.
  ///   - corner: Its corners' radius.
  public init(picture: DesktopPicture, saturation: Double, corner: CGFloat) {
    self.picture = picture
    self.saturation = saturation
    self.corner = corner
  }
}
