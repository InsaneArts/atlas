import AtlasService
import Foundation
import Testing

@testable import AtlasCommands

@Suite("LifecycleReading")
struct LifecycleReadingTests {
  /// A request reply of `kind`.
  private static func reply(kind: String) -> String {
    #"{"ok": true, "request": {"id": "\#(String(repeating: "a", count: 48))", "kind": "\#(kind)", "name": "work", "vm": "\#(String(repeating: "b", count: 24))", "settings": "~/.config/atlas.toml", "session": "Claude Code in atlas", "until": 1700000600000}}"#
  }

  /// What `reply` reads as, with exit status `status`.
  private static func reading(_ reply: String, status: Int32 = 0) -> LifecycleReading {
    LifecycleReading.of(Outcome.parse(status: status, stdout: Data(reply.utf8), stderr: Data()))
  }

  @Test(
    "a request of each kind reads whole, its deadline from milliseconds",
    arguments: [("add", LifecycleKind.add), ("remove", .remove), ("uninstall", .uninstall)]
  )
  func eachKind(word: String, kind: LifecycleKind) {
    let expected = LifecycleRequest(
      id: String(repeating: "a", count: 48),
      kind: kind,
      name: "work",
      vm: String(repeating: "b", count: 24),
      settings: "~/.config/atlas.toml",
      session: "Claude Code in atlas",
      until: Date(timeIntervalSince1970: 1_700_000_600)
    )
    #expect(Self.reading(Self.reply(kind: word)) == .request(expected))
  }

  @Test(
    "a request answered before reads with what was answered",
    arguments: [("allowed", LifecycleAnswered.allowed), ("refused", .refused), ("used", .used)]
  )
  func answered(word: String, answered: LifecycleAnswered) {
    let reply = Self.reply(kind: "add")
      .replacingOccurrences(of: "\"until\": 1700000600000", with: "\"until\": 1700000600000, \"answered\": \"\(word)\"")
    guard case .request(let request) = Self.reading(reply) else {
      Issue.record("the request did not read")
      return
    }
    #expect(request.answered == answered)
    guard case .request(let unanswered) = Self.reading(Self.reply(kind: "add")) else {
      Issue.record("the request did not read")
      return
    }
    #expect(unanswered.answered == nil)
  }

  @Test("a refused read says the command line's own sentence")
  func refused() {
    let reply = #"{"ok": false, "error": "This request has expired."}"#
    #expect(Self.reading(reply, status: 1) == .failed("This request has expired."))
  }

  @Test("a request of a kind this app does not know is never shown")
  func unknownKind() {
    #expect(Self.reading(Self.reply(kind: "rename")) == .failed(LifecycleReading.unreadable))
  }
}
