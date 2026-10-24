/// The ⋯ menu as the presenter decides it: which picture choice is ticked, whether there is a picture to copy or save,
/// which of the helper's commands it offers, and the note at its foot.
struct ViewerMoreMenu: Equatable {
  /// The picture choice ticked.
  var picture: ViewerPictureChoice
  /// Whether a picture shows, to copy or save.
  var hasPicture: Bool
  /// Which of the helper's commands it offers.
  var helper: HelperMenuState
  /// The note at its foot: the helper and its display.
  var note: String
}
