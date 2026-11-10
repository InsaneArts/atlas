import Foundation
import Testing

@testable import AtlasApp

@Suite("LaunchInput")
struct LaunchInputTests {
  /// Lines a test hands out one at a time, then holds input open, as a parent that writes nothing more does.
  nonisolated final class OpenInput: Sendable {
    /// The lines, then a wait that never ends until the test lets go.
    private let lines: [String]
    /// Counts the lines handed out, under its lock.
    private let handed = NSLock()
    /// The next line's place. Touched only under `handed`.
    nonisolated(unsafe) private var place = 0
    /// Held while input stays open.
    private let open = DispatchSemaphore(value: 0)

    /// Input that gives `lines`, then stays open.
    ///
    /// - Parameter lines: The lines.
    init(_ lines: [String]) {
      self.lines = lines
    }

    /// The next line, or a wait until `close()`, then none.
    ///
    /// - Returns: The line, or nil once closed.
    func read() -> String? {
      handed.lock()
      let index = place
      place += 1
      handed.unlock()
      if index < lines.count { return lines[index] }
      open.wait()
      return nil
    }

    /// Closes input.
    func close() {
      open.signal()
    }
  }

  @Test("the URL then the secret are read in turn, and the next line waits for the app to listen")
  func theLinesComeInTurn() {
    let open = OpenInput(["http://127.0.0.1:1/a/", "secret \(String(repeating: "ab", count: 32))", "viewer"])
    let input = LaunchInput(read: open.read)
    #expect(input.next() == .line("http://127.0.0.1:1/a/"))
    #expect(input.next(within: 2) == .line("secret \(String(repeating: "ab", count: 32))"))
    #expect(input.next(within: 2) == .line("viewer"))
    open.close()
    #expect(input.next() == .closed)
  }

  @Test("a launch whose parent writes no secret and keeps input open goes on once the wait ends")
  func theWaitForTheSecretEnds() {
    let open = OpenInput(["http://127.0.0.1:1/a/"])
    let input = LaunchInput(read: open.read)
    #expect(input.next() == .line("http://127.0.0.1:1/a/"))
    let started = Date()
    #expect(input.next(within: 0.2) == .timedOut)
    #expect(Date().timeIntervalSince(started) < AtlasApp.secretWait, "the launch never blocks on it")
    open.close()
    #expect(input.next() == .closed)
  }
}
