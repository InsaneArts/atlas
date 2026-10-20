/// The helper's desktop at its own shape, and what is laid over it.
package struct ViewerStageState: Equatable, Sendable {
  /// The latest picture of the helper's desktop; nil shows the placeholder.
  package var picture: DesktopPicture?
  /// Whether the picture is dimmed: it is paused, so it reads as not live.
  package var isDimmed: Bool
  /// Whether this window holds control, which the stage's accent edge and glow show.
  package var isInControl: Bool
  /// Why there is no picture, in place of one; nil while a picture shows.
  package var placeholder: ViewerStagePlaceholder?
  /// The status chip at the top of the stage; nil shows none.
  package var chip: ViewerStageChip?
  /// One line about the last thing that happened, under the chip; nil shows none.
  package var notice: String?
  /// The latest actions over the desktop's corner, oldest first.
  package var captions: [ActivityEntry]

  /// Creates the stage's state.
  ///
  /// - Parameters:
  ///   - picture: The latest picture of the helper's desktop; nil shows the placeholder.
  ///   - isDimmed: Whether the picture is dimmed: it is paused, so it reads as not live.
  ///   - isInControl: Whether this window holds control, which the stage's accent edge and glow show.
  ///   - placeholder: Why there is no picture, in place of one; nil while a picture shows.
  ///   - chip: The status chip at the top of the stage; nil shows none.
  ///   - notice: One line about the last thing that happened, under the chip; nil shows none.
  ///   - captions: The latest actions over the desktop's corner, oldest first.
  package init(
    picture: DesktopPicture? = nil,
    isDimmed: Bool,
    isInControl: Bool,
    placeholder: ViewerStagePlaceholder? = nil,
    chip: ViewerStageChip? = nil,
    notice: String? = nil,
    captions: [ActivityEntry]
  ) {
    self.picture = picture
    self.isDimmed = isDimmed
    self.isInControl = isInControl
    self.placeholder = placeholder
    self.chip = chip
    self.notice = notice
    self.captions = captions
  }
}
