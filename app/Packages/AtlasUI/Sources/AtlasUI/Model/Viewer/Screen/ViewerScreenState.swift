import CoreGraphics

/// The Screen tab: the helper's screen at its own shape, or why there is none, and what is laid over it.
public struct ViewerScreenState: Equatable, Sendable {
  /// The latest picture of the helper's screen; nil shows ``empty``.
  public var picture: DesktopPicture?
  /// How the picture is drawn: live, dimmed, paused or inside control's rim.
  public var look: PictureLook
  /// Why there is no picture, in place of one; nil while a picture shows.
  public var empty: ViewerEmpty?
  /// The chip at the top of the screen; nil shows none.
  public var chip: ViewerScreenChip?
  /// One line about the last thing that happened, under the chip; nil shows none.
  public var notice: String?
  /// The latest actions in the screen's corner, oldest first.
  public var captions: [ActivityEntry]
  /// The helper's display in points while the screen is drawn at actual size, one view point a display point, in a
  /// view that scrolls; nil fits the screen to the window.
  public var actualSize: CGSize?
  /// A window shown from the Apps tab, outlined over the picture; nil shows none.
  public var outline: ViewerWindowOutline?

  /// Creates the Screen tab's state.
  ///
  /// - Parameters:
  ///   - picture: The latest picture of the helper's screen; nil shows `empty`.
  ///   - look: How the picture is drawn.
  ///   - empty: Why there is no picture, in place of one.
  ///   - chip: The chip at the top of the screen.
  ///   - notice: One line about the last thing that happened.
  ///   - captions: The latest actions in the screen's corner, oldest first.
  ///   - actualSize: The helper's display in points while drawn at actual size; nil fits it to the window.
  ///   - outline: A window shown from the Apps tab; nil shows none.
  public init(
    picture: DesktopPicture? = nil,
    look: PictureLook = .live,
    empty: ViewerEmpty? = nil,
    chip: ViewerScreenChip? = nil,
    notice: String? = nil,
    captions: [ActivityEntry] = [],
    actualSize: CGSize? = nil,
    outline: ViewerWindowOutline? = nil
  ) {
    self.picture = picture
    self.look = look
    self.empty = empty
    self.chip = chip
    self.notice = notice
    self.captions = captions
    self.actualSize = actualSize
    self.outline = outline
  }
}
