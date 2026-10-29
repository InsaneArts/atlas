/// A process that left a stopped background job's group and still ran, which Atlas never signals: its process id and
/// command name, as the helper's kernel keeps it.
public struct CleanupProcess: Decodable, Equatable, Sendable {
  /// Its process id.
  public var pid: Int
  /// Its command name, such as "sleep".
  public var name: String

  /// Creates a process.
  ///
  /// - Parameters:
  ///   - pid: Its process id.
  ///   - name: Its command name.
  public init(pid: Int, name: String) {
    self.pid = pid
    self.name = name
  }
}
