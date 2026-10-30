import AtlasService
import AtlasUI

/// Selects an icon by authoritative bundle identity, using names only while that identity is unknown.
enum AppIconSelection {
  /// The permitted keys, with a learned bundle before a name for an app of unknown identity.
  ///
  /// - Parameters:
  ///   - bundleId: The explicit bundle identifier, if valid.
  ///   - name: The display name.
  ///   - names: Bundle identifiers learned for name keys.
  /// - Returns: Only the explicit bundle key when known, otherwise the name's fallback keys.
  static func keys(bundleId: String?, name: String, names: [String: String] = [:]) -> [AppIconKey] {
    if let explicit = AppIconKey(bundleId: bundleId) { return [explicit] }
    let byName = AppIconKey(name: name)
    let learned = byName.flatMap { names[$0.text] }
      .flatMap { AppIconKey(bundleId: $0) }
    return [learned, byName].compactMap { $0 }
  }

  /// The first image permitted by the app's identity.
  ///
  /// - Parameters:
  ///   - bundleId: The explicit bundle identifier, if known.
  ///   - name: The display name.
  ///   - images: Images by icon key.
  ///   - names: Bundle identifiers learned for name keys.
  /// - Returns: The selected image, or nil.
  static func image(
    bundleId: String?,
    name: String,
    images: [String: IconImage],
    names: [String: String] = [:]
  ) -> IconImage? {
    keys(bundleId: bundleId, name: name, names: names)
      .lazy
      .compactMap { images[$0.text] }
      .first
  }
}
