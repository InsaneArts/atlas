import AtlasService
import Foundation

/// An app whose icon is wanted, with what may find it: its key, its bundle identifier and its name.
nonisolated struct AppIconWant: Equatable, Sendable {
  /// What the icon lands under.
  var key: AppIconKey
  /// The app's bundle identifier, when known.
  var bundleId: String?
  /// The app's name, when known.
  var name: String?

  /// What is wanted for an app an action named; nil for one with neither a bundle identifier nor a name it may be
  /// looked up by.
  ///
  /// - Parameters:
  ///   - bundleId: The app's bundle identifier, as the row has it.
  ///   - name: The app's name, as the row has it.
  /// - Returns: The want, or nil.
  static func named(bundleId: String?, name: String?) -> AppIconWant? {
    let id = AppIconKey(bundleId: bundleId)?.bundleId
    guard let key = AppIconKey(bundleId: id) ?? AppIconKey(name: name) else { return nil }
    return AppIconWant(key: key, bundleId: id, name: name)
  }
}
