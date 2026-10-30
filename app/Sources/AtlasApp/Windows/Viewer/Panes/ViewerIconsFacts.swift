import AtlasUI

/// The apps' icons the window has found, which Activity and Apps draw: each by what it is known by, and the bundle
/// identifier each app's name was seen with, so a row that names only an app finds the icon found for its bundle.
struct ViewerIconsFacts {
  /// The current pane's icons, retained by key independently of the shared repository's cache eviction.
  var images: [String: IconImage] = [:]
  /// The bundle identifier each app's name was seen with, by the name's key's text.
  var names: [String: String] = [:]

  /// The icon found for an app, if any.
  ///
  /// - Parameters:
  ///   - bundleId: The app's bundle identifier, if known.
  ///   - name: The app's name.
  /// - Returns: The icon, or nil.
  func image(bundleId: String?, name: String) -> IconImage? {
    AppIconSelection.image(bundleId: bundleId, name: name, images: images, names: names)
  }
}
