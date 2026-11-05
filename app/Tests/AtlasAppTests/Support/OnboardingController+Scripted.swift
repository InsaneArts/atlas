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
  ///   - mark: Where its login timing's markers go; nowhere by default, never this Mac's log.
  ///   - console: The scripted session reading; on-console by default, independent of the test machine.
  ///   - consoleUser: Who has the Mac's screen; this login, `me`, by default.
  /// - Returns: The controller, which reads no icon or picture from this Mac and reveals what it shows in the Finder
  ///   into `opened`.
  static func scripted(
    _ commands: ScriptedCommands,
    opened: OpenedURLs = OpenedURLs(),
    loginItem: ScriptedLoginItem = ScriptedLoginItem(),
    installs: AgentInstalls? = nil,
    clock: ManualClock = ManualClock(),
    mark: @escaping @MainActor (String) -> Void = { _ in },
    console: SessionConsole = SessionConsole(read: { true }),
    consoleUser: @escaping @MainActor () -> String? = { "me" }
  ) -> OnboardingController {
    OnboardingController(
      dependencies: OnboardingDependencies(
        commands: commands,
        console: console,
        consoleUser: consoleUser,
        openURL: { opened.urls.append($0) },
        loginItem: loginItem.seam,
        installs: installs ?? AgentInstalls(commands: commands),
        clock: clock,
        mark: mark,
        agentIcons: { [:] },
        accountPicture: { _ in nil },
        reveal: { opened.urls.append($0) }
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

  /// Runs `act` the moment the queue takes its next request, before the request's work starts, and waits for that
  /// taking. `act` runs inside the queue's own turn of the main actor, which the work's start waits behind, so it
  /// always lands between the taking and the start. Polling the queue from the test, a turn at a time, could not
  /// promise that: the order in which the main actor runs the test's turn and the work's start is not fixed.
  ///
  /// - Parameters:
  ///   - sourceLocation: Where a failure is reported: the caller's line.
  ///   - act: What happens between the taking and the start, such as Back.
  /// - Returns: The request's work, which may have run by the time this returns.
  /// - Throws: The failed requirement, when the queue took no request.
  func untilTaken(
    sourceLocation: SourceLocation = #_sourceLocation,
    acting act: @escaping @MainActor () -> Void
  ) async throws -> Task<Void, Never> {
    let work = await withCheckedContinuation { (taken: CheckedContinuation<Task<Void, Never>?, Never>) in
      taking = { [weak self] _ in
        self?.taking = nil
        let work = self?.performing?.work
        act()
        taken.resume(returning: work)
      }
    }
    return try #require(work, "the queue took no request", sourceLocation: sourceLocation)
  }

  /// Waits for the window's first reading of the facts to land.
  ///
  /// - Parameter sourceLocation: Where a failure is reported: the caller's line.
  /// - Throws: The failed requirement, when the facts are never read.
  func untilLoaded(sourceLocation: SourceLocation = #_sourceLocation) async throws {
    try await settle("the facts were never read", sourceLocation: sourceLocation) { snapshot.loaded }
  }

  /// Reads until this login has been found on screen ``OnboardingController/settledReadings`` times in a row, as the
  /// end-to-end check waits for: each reading after the first is asked for at once, a bounded number of times.
  ///
  /// - Parameters:
  ///   - commands: The window's commands, whose readings are counted.
  ///   - sourceLocation: Where a failure is reported: the caller's line.
  /// - Throws: The failed requirement, when a reading never comes.
  func untilSettled(_ commands: ScriptedCommands, sourceLocation: SourceLocation = #_sourceLocation) async throws {
    try await untilLoaded(sourceLocation: sourceLocation)
    for _ in 1..<Self.settledReadings {
      let readings = commands.ran.count
      refresh()
      try await settle("the next reading never came", sourceLocation: sourceLocation) {
        commands.ran.count > readings
      }
    }
  }

}
