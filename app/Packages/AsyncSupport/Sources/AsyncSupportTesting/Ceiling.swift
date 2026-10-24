import Testing

/// How long a test waits on background work before it fails; every Atlas test target shares it.
public enum Ceiling {
  /// The longest a test waits for background work before it fails: long beside the work it waits for, so a loaded
  /// machine never reaches it while the rule holds. A shared CI runner, running every suite at once, takes well over
  /// two seconds.
  public static let work: Duration = .seconds(10)

  /// Runs `passed` once `ceiling` has passed on the wall clock, unless cancelled first: the bound of every wait here.
  ///
  /// It is the one wall-clock wait in the tests, and a bound, never a pause: a wait whose event comes cancels it, so
  /// a test that passes spends no time on it.
  ///
  /// - Parameters:
  ///   - ceiling: How long to wait.
  ///   - passed: What to do once it has passed.
  /// - Returns: The timer, which the wait cancels once its event comes.
  static func timer(_ ceiling: Duration, _ passed: @escaping @Sendable () -> Void) -> Task<Void, Never> {
    // Unstructured, so it runs beside the wait it bounds and outlives none: the wait cancels it as it returns.
    Task {
      do {
        try await Task.sleep(for: ceiling)
      } catch {
        return
      }
      passed()
    }
  }
}
