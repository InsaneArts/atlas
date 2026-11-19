/// The power changes a test's Settings asked of the service, in order, as "ID on" or "ID off".
@MainActor
final class PowerLog {
  /// Each change, in order.
  var calls: [String] = []
}
