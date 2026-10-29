import CoreGraphics

/// What the live picture's slot draws: the newest picture, with the colour its look keeps and the corners it
/// is cut to, and whether it is moving between the stage and the small screen. The app draws it through a layer,
/// previews and renders through an image (``StillPicture``); the black that darkens a look is laid over either by the
/// screen.
public struct ViewerLivePicture: Equatable, Sendable {
  /// The picture.
  public var picture: DesktopPicture
  /// How much colour it keeps, 1 for all of it.
  public var saturation: Double
  /// Its corners' radius.
  public var corner: CGFloat
  /// Whether the picture is moving between the stage and the small screen, so a change of its corners follows the
  /// zoom's spring (``Motion/LayerSpring/zoom``) rather than landing at once. False under Reduce Motion, where the
  /// picture fades in place instead.
  public var zooms: Bool

  /// Creates what the slot draws.
  ///
  /// - Parameters:
  ///   - picture: The picture.
  ///   - saturation: How much colour it keeps.
  ///   - corner: Its corners' radius.
  ///   - zooms: Whether it is moving between the stage and the small screen; false by default.
  public init(picture: DesktopPicture, saturation: Double, corner: CGFloat, zooms: Bool = false) {
    self.picture = picture
    self.saturation = saturation
    self.corner = corner
    self.zooms = zooms
  }
}
