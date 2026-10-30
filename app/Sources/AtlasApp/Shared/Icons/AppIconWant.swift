import AtlasService
import Foundation

/// An app whose icon is wanted, with all that may find it: its key, its bundle identifier and name, and, for an app
/// open now, its bundle where it is installed and the icon the service sent. Never a copy's path, which
/// ``HelperApp/installedBundle(home:)`` leaves out.
nonisolated struct AppIconWant: Equatable, Sendable {
  /// What the icon lands under.
  var key: AppIconKey
  /// The app's bundle identifier, when known.
  var bundleId: String?
  /// The app's name, when known.
  var name: String?
  /// The app's bundle where apps are installed, while it is open, to read its icon from first.
  var bundle: String?
  /// The icon the service sent for the open app, as PNG, for when the bundle does not read.
  var served: Data?

  /// Whether this request carries evidence from an open app.
  var isLive: Bool { bundle != nil || served != nil }

  /// Whether this request already includes another request's live evidence for the same identity.
  ///
  /// - Parameter other: The incoming evidence.
  /// - Returns: Whether processing it would add no bundle or service icon.
  func covers(_ other: AppIconWant) -> Bool {
    key == other.key && (other.bundle == nil || bundle == other.bundle)
      && (other.served == nil || served == other.served)
  }

  /// Fills absent evidence from an earlier request without replacing newer evidence.
  ///
  /// - Parameter earlier: Work already queued or being processed for this identity.
  /// - Returns: The combined request.
  func keepingEvidence(from earlier: AppIconWant) -> AppIconWant {
    guard key == earlier.key else { return self }
    var combined = self
    combined.bundle = bundle ?? earlier.bundle
    combined.served = served ?? earlier.served
    return combined
  }

  /// What is wanted for an app open now, by its key; nil for an app with neither a bundle identifier nor a name.
  ///
  /// - Parameters:
  ///   - app: The app.
  ///   - home: The person's home folder.
  ///   - live: Whether to read it from the app itself, when its bundle or service evidence has changed.
  /// - Returns: The want, or nil.
  static func open(_ app: HelperApp, home: String, live: Bool) -> AppIconWant? {
    guard let key = app.iconKey else { return nil }
    return AppIconWant(
      key: key,
      bundleId: AppIconKey(bundleId: app.bundleId)?.bundleId,
      name: app.name.isEmpty ? nil : app.name,
      bundle: live ? app.installedBundle(home: home) : nil,
      served: live ? app.icon : nil
    )
  }

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
