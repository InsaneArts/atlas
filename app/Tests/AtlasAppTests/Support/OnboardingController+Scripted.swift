import AsyncSupportTesting
import AtlasCommandsTesting
import Testing

@testable import AtlasApp

extension OnboardingController {
  /// A setup window's controller that runs `commands` and reaches nothing else of the system.
  ///
  /// - Parameters:
  ///   - commands: The scripted command line.
  ///   - opened: Records the URLs opened.
  ///   - loginItem: The scripted login item.
  ///   - installs: The agent installs it shares with another window; its own over `commands` when nil.
  ///   - clock: The clock its waits run on; by default one nobody advances.
  /// - Returns: The controller.
  static func scripted(
    _ commands: ScriptedCommands,
    opened: OpenedURLs = OpenedURLs(),
    loginItem: ScriptedLoginItem = ScriptedLoginItem(),
    installs: AgentInstalls? = nil,
    clock: ManualClock = ManualClock()
  ) -> OnboardingController {
    OnboardingController(
      dependencies: OnboardingDependencies(
        commands: commands,
        openURL: { opened.urls.append($0) },
        loginItem: loginItem.seam,
        installs: installs ?? AgentInstalls(commands: commands),
        clock: clock
      )
    )
  }

  /// Runs `body` while the window is open: the controller's `run()`, as showing the window starts it, which stops once
  /// `body` ends, however it ends.
  ///
  /// - Parameter body: What the test does while the window is open, given `run()`'s task, which ends by itself once
  ///   the window has closed and a request under way has settled.
  /// - Returns: What `body` returns.
  /// - Throws: What `body` throws.
  func whileOpen<Value>(_ body: (Task<Void, Never>) async throws -> Value) async rethrows -> Value {
    // Unstructured, so a test can wait for run() to end by itself; cancelled once the body ends.
    let running = Task { await self.run() }
    defer { running.cancel() }
    return try await body(running)
  }

  /// Waits for the window's first reading of the facts to land.
  ///
  /// - Parameter sourceLocation: Where a failure is reported: the caller's line.
  /// - Throws: The failed requirement, when the facts are never read.
  func untilLoaded(sourceLocation: SourceLocation = #_sourceLocation) async throws {
    try await settle("the facts were never read", sourceLocation: sourceLocation) { snapshot.loaded }
  }
}
