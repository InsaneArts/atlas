import Testing

/// Suites that use AppKit's process-wide menus, windows, focus, rendering or notifications share one gate.
/// `.serialized` orders a suite's cases; this trait also excludes cases in other AppKit suites across `await`.
nonisolated struct AppKitState: SuiteTrait, TestTrait, TestScoping {
  /// Applies the gate to every test in the suite.
  var isRecursive: Bool { true }

  /// Gates individual cases, never a suite or parameterized test enclosing cases that need the same gate.
  ///
  /// - Parameters:
  ///   - test: The suite or test being scoped.
  ///   - testCase: The individual invocation, when one is running.
  /// - Returns: This gate only for an individual invocation.
  func scopeProvider(for test: Test, testCase: Test.Case?) -> Self? {
    testCase == nil ? nil : self
  }

  /// Holds the shared gate through the test's body and deferred cleanup, including a thrown failure.
  ///
  /// - Parameters:
  ///   - test: The test being run.
  ///   - testCase: Its individual invocation.
  ///   - function: The test and its cleanup.
  /// - Throws: The test's failure, after releasing the gate.
  @concurrent
  func provideScope(
    for test: Test,
    testCase: Test.Case?,
    performing function: @concurrent @Sendable () async throws -> Void
  )
    async throws
  {
    try await AppKitTestGate.shared
      .withOwnership(function)
  }
}
