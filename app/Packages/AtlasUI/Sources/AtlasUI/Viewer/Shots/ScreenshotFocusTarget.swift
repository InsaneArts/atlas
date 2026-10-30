/// The two screenshot surfaces share a focus destination so closing a large picture returns keys to its grid.
enum ScreenshotFocusTarget: Hashable {
  /// The screenshot tiles, which take focus again after closing a large picture.
  case grid
  /// The large picture alone, with its buttons owning their native activation keys.
  case large
}
