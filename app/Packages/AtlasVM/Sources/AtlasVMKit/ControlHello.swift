/// What a VM's runner declares to its station: a version, and the names of
/// what it can do. A station uses only what is declared.
public struct ControlHello: Codable, Equatable, Sendable {
  /// The control interface's version.
  public var version: Int
  /// What it can do, by name.
  public var capabilities: [String]

  /// What this runner declares.
  public static let ours = ControlHello(
    version: 1,
    capabilities: ["changes", "outcomes", "power", "window-input", "install", "install-resume"]
  )
}
