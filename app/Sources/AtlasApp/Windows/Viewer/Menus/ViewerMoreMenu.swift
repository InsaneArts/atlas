/// The ⋯ menu as the presenter decides it: what Copy and Save act on, whether the screen draws at actual size, and how
/// often pictures arrive, which it ticks even while the picture is paused, as the rate it resumes at.
struct ViewerMoreMenu: Equatable {
  /// Whether a picture shows, to copy, save or draw at actual size.
  var hasPicture: Bool
  /// Whether Copy and Save act on a screenshot, shown large or selected on the Screenshots tab, rather than the picture.
  var actsOnShot: Bool
  /// Whether the screen draws at actual size.
  var actualSize: Bool
  /// How often pictures arrive, or will once the picture resumes.
  var rate: ViewerPictureRate
}
