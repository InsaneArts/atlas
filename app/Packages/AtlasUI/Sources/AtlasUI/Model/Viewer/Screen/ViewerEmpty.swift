/// Why the screen shows no picture, in place of one (v3 V7): the message, what its one button sends, and, while
/// connecting again, the last picture kept behind it and whether the ring round the mark shows yet.
public struct ViewerEmpty: Equatable, Sendable {
  /// What the message says.
  public var message: EmptyMessageState
  /// What its button sends; nil when it has none.
  public var event: ViewerEvent?
  /// The last picture shown, kept behind the message, blurred and dimmed, while the viewer connects again (v3 X4);
  /// nil for the plain stage.
  public var lastPicture: DesktopPicture?
  /// Whether the ring traces round the mark: once the viewer has waited ``Motion/Duration/appearDelay`` for a
  /// picture, so a quick answer never flashes it (v3 R12).
  public var showsRing: Bool

  /// Creates the screen's empty message.
  ///
  /// - Parameters:
  ///   - message: What the message says.
  ///   - event: What its button sends; nil when it has none.
  ///   - lastPicture: The last picture shown, kept behind the message; nil, by default, for none.
  ///   - showsRing: Whether the ring traces round the mark; not by default.
  public init(
    message: EmptyMessageState,
    event: ViewerEvent? = nil,
    lastPicture: DesktopPicture? = nil,
    showsRing: Bool = false
  ) {
    self.message = message
    self.event = event
    self.lastPicture = lastPicture
    self.showsRing = showsRing
  }
}
