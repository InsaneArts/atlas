import AtlasService

/// A window shown on the screen from the Apps tab, as it was when Show on screen was pressed: view-only, it does not
/// follow the window should it move.
struct ViewerShownWindow {
  /// The app's name.
  var app: String
  /// The app's bundle identifier, which its icon is found by; nil when unknown.
  var bundleId: String?
  /// The window, with its place on the helper's display.
  var window: HelperWindow
}
