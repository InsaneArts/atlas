import AtlasService
import AtlasUI

/// The apps' icons the window has found, which Activity and Apps draw: each by what it is known by, and the bundle
/// identifier each app's name was seen with, so a row that names only an app finds the icon found for its bundle.
struct ViewerIconsFacts {
  /// The icons found, by their key's text (``AppIconKey/text``).
  var images: [String: IconImage] = [:]
  /// The bundle identifier each app's name was seen with, by the name's key's text.
  var names: [String: String] = [:]

  /// The keys an app's icon may be under, the surest first: its bundle identifier, the one its name was seen with,
  /// then its name.
  ///
  /// - Parameters:
  ///   - bundleId: The app's bundle identifier, if known.
  ///   - name: The app's name.
  /// - Returns: The keys.
  func keys(bundleId: String?, name: String) -> [AppIconKey] {
    let byName = AppIconKey(name: name)
    let seen = byName.flatMap { names[$0.text] }
      .flatMap { AppIconKey(bundleId: $0) }
    return [AppIconKey(bundleId: bundleId), seen, byName].compactMap { $0 }
  }

  /// The icon found for an app, if any.
  ///
  /// - Parameters:
  ///   - bundleId: The app's bundle identifier, if known.
  ///   - name: The app's name.
  /// - Returns: The icon, or nil.
  func image(bundleId: String?, name: String) -> IconImage? {
    keys(bundleId: bundleId, name: name)
      .lazy
      .compactMap { images[$0.text] }
      .first
  }
}
