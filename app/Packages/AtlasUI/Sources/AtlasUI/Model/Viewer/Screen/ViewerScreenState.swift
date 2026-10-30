import CoreGraphics

/// The Screen: the helper's screen at its own shape, or why there is none, a window outlined over it, and the commands
/// its context menu offers.
public struct ViewerScreenState: Equatable, Sendable {
  /// The latest picture of the helper's screen; nil shows ``empty``.
  public var picture: DesktopPicture?
  /// How the picture is drawn: live, dimmed, paused or inside control's edge.
  public var look: PictureLook
  /// Why there is no picture, in place of one; nil while a picture shows.
  public var empty: ViewerEmpty?
  /// The helper's display in points while the screen is drawn at actual size, one view point a display point, in a
  /// view that scrolls; nil fits the screen to the card.
  public var actualSize: CGSize?
  /// A window shown from the Apps list, outlined over the picture; nil shows none.
  public var outline: ViewerWindowOutline?
  /// What a right click on the picture offers, in sections; none while this window holds control, where the click is
  /// the helper's.
  public var menu: [ViewerMenuSection]

  /// Creates the Screen's state.
  ///
  /// - Parameters:
  ///   - picture: The latest picture of the helper's screen; nil shows `empty`.
  ///   - look: How the picture is drawn.
  ///   - empty: Why there is no picture, in place of one.
  ///   - actualSize: The helper's display in points while drawn at actual size; nil fits it to the card.
  ///   - outline: A window shown from the Apps list; nil shows none.
  ///   - menu: What a right click on the picture offers; nothing by default.
  public init(
    picture: DesktopPicture? = nil,
    look: PictureLook = .live,
    empty: ViewerEmpty? = nil,
    actualSize: CGSize? = nil,
    outline: ViewerWindowOutline? = nil,
    menu: [ViewerMenuSection] = []
  ) {
    self.picture = picture
    self.look = look
    self.empty = empty
    self.actualSize = actualSize
    self.outline = outline
    self.menu = menu
  }
}
