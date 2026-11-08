/// What acts on the window's picture, as the presenter decides it: what Copy and Save act on, whether the screen draws
/// at actual size, and how often pictures arrive, which it ticks even while the picture is paused, as the rate it
/// resumes at. The stage's context menu and the main menu's items both follow it.
struct ViewerPictureMenu: Equatable {
  /// Whether a picture shows, to copy, save or draw at actual size.
  var hasPicture: Bool
  /// Whether Copy and Save act on the picture sheet's picture rather than the screen's.
  var actsOnShot: Bool
  /// Whether the screen draws at actual size.
  var actualSize: Bool
  /// How often pictures arrive, or will once the picture resumes.
  var rate: ViewerPictureRate
}
