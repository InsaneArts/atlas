/// A release of macOS: its version, as 27.2.1, and its build, as 27C101.
public struct MacOSRelease: Codable, Equatable, Sendable {
  /// Its version, its numbers joined by dots; a last zero left out, as
  /// macOS itself says it.
  public var version: String
  /// Its build.
  public var build: String

  /// The release `version` of `build`.
  public init(version: String, build: String) {
    self.version = version
    self.build = build
  }

  /// The release `major`.`minor`.`patch` of `build`, its patch left out
  /// when it is zero.
  public init(major: Int, minor: Int, patch: Int, build: String) {
    let numbers = patch == 0 ? [major, minor] : [major, minor, patch]
    self.init(version: numbers.map(String.init).joined(separator: "."), build: build)
  }
}
