/// What `atlas update --check` reports: the version installed, the newest published, and whether that one is newer.
/// The command line decides which is newer; the app only reads its answer.
package struct UpdateCheck: Decodable, Sendable {
  /// The version installed, such as `0.1.0`.
  package let version: String?
  /// The newest version published, such as `0.2.0`.
  package let latest: String?
  /// Whether the newest published version is newer than the one installed.
  package let newer: Bool?
}
