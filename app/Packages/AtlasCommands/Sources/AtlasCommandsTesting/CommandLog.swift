/// What the scripted service had been asked when a scripted command ran, so a test checks the order of the two.
@MainActor
public final class CommandLog {
  /// The service's control and input requests made before each command, one list a command.
  public var serviceLogs: [[String]] = []

  /// A log of no commands yet.
  public init() {}
}
