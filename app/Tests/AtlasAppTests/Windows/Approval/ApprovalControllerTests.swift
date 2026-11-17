import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasUI
import Foundation
import Observation
import Testing

@testable import AtlasApp

@Suite("ApprovalController")
struct ApprovalControllerTests {
  /// The request's ID.
  private static let id = String(repeating: "a", count: 48)
  /// The moment the tests take for now.
  private static let now = Date(timeIntervalSince1970: 1_700_000_000)
  /// How long an answer kept holds before the window closes.
  private static let hold = Duration.seconds(Motion.Duration.doneHold.seconds)

  /// How often the window closed; observable, so a test's `settle` hears each close.
  @Observable
  final class Closes {
    /// How often the window closed.
    var count = 0
  }

  /// A reply reading a request of `kind`, `seconds` from now to answer.
  private static func request(kind: String = "add", seconds: Int = 600, answered: String? = nil) -> String {
    let until = Int64(now.timeIntervalSince1970 * 1000) + Int64(seconds) * 1000
    let vm = String(repeating: "b", count: 24)
    let answer = answered.map { #", "answered": "\#($0)""# } ?? ""
    return #"{"ok": true, "request": {"id": "\#(id)", "kind": "\#(kind)", "name": "work", "vm": "\#(vm)", "#
      + #""settings": "~/.config/atlas.toml", "session": "a terminal", "until": \#(until)\#(answer)}}"#
  }

  /// A controller over scripted commands, its time `now`.
  private static func controller(
    _ commands: ScriptedCommands,
    closes: Closes,
    clock: ManualClock
  ) -> ApprovalController {
    ApprovalController(
      id: id,
      commands: commands,
      links: ApprovalLinks(close: { closes.count += 1 }, now: { now }, clock: clock)
    )
  }

  /// The answers the scripted command line was asked to keep, as their last argument.
  private static func answers(_ commands: ScriptedCommands) -> [String] {
    commands.ran
      .filter { $0.prefix(2) == ["lifecycle", "answer"] }
      .compactMap(\.last)
  }

  @Test("a request read shows what it asks, for which VM, from which session")
  func aRequestShows() async {
    let commands = ScriptedCommands(["lifecycle request": Self.request()])
    let controller = Self.controller(commands, closes: Closes(), clock: ManualClock())
    await controller.start()
    #expect(commands.ran.first == ["lifecycle", "request", "--id", Self.id])
    #expect(controller.state == ApprovalState(operation: .add, desktop: "work", session: "a terminal", phase: .asking))
  }

  @Test("Allow sends --allow once however often it is pressed, holds, then closes")
  func allowSendsOnce() async throws {
    let commands = ScriptedCommands([
      "lifecycle request": Self.request(),
      "lifecycle answer": #"{"ok": true, "answer": "allowed"}"#,
    ])
    let closes = Closes()
    let clock = ManualClock()
    let controller = Self.controller(commands, closes: closes, clock: clock)
    await controller.start()
    let first = Task { await controller.answer(allow: true) }
    let second = Task { await controller.answer(allow: true) }
    try await clock.untilSleeping(for: Self.hold, "the kept answer never held")
    #expect(controller.phase == .answered(allowed: true))
    clock.advance(by: Self.hold)
    await first.value
    await second.value
    #expect(Self.answers(commands) == ["--allow"])
    #expect(closes.count == 1)
  }

  @Test("closing the window before an answer refuses the request, then closes")
  func closingRefuses() async throws {
    let commands = ScriptedCommands([
      "lifecycle request": Self.request(),
      "lifecycle answer": #"{"ok": true, "answer": "refused"}"#,
    ])
    let closes = Closes()
    let clock = ManualClock()
    let controller = Self.controller(commands, closes: closes, clock: clock)
    await controller.start()
    controller.send(.close)
    try await clock.untilSleeping(for: Self.hold, "the refusal never held")
    #expect(controller.phase == .answered(allowed: false))
    controller.send(.close)
    try await settle("Done never closed the window") { closes.count == 1 }
    #expect(Self.answers(commands) == ["--refuse"])
  }

  @Test("a request whose time runs out shows expired and is never answered")
  func expiryShows() async throws {
    let commands = ScriptedCommands([
      "lifecycle request": Self.request(seconds: 60),
      "lifecycle answer": #"{"ok": true, "answer": "allowed"}"#,
    ])
    let closes = Closes()
    let clock = ManualClock()
    let controller = Self.controller(commands, closes: closes, clock: clock)
    await controller.start()
    try await clock.untilSleeping(for: .seconds(60), "the request's expiry never waited")
    clock.advance(by: .seconds(60))
    await afterQueuedWork()
    #expect(controller.phase == .expired)
    await controller.answer(allow: true)
    #expect(Self.answers(commands).isEmpty)
    controller.send(.close)
    #expect(closes.count == 1)
  }

  @Test(
    "a request answered before reopens as answered, past its time too, and is never answered again",
    arguments: [
      ("allowed", ApprovalEarlierAnswer.allowed),
      ("refused", .refused),
      ("used", .used),
    ]
  )
  func answeredEarlier(word: String, answer: ApprovalEarlierAnswer) async {
    let commands = ScriptedCommands([
      "lifecycle request": Self.request(seconds: -1, answered: word),
      "lifecycle answer": #"{"ok": true, "answer": "allowed"}"#,
    ])
    let closes = Closes()
    let controller = Self.controller(commands, closes: closes, clock: ManualClock())
    await controller.start()
    #expect(controller.phase == .answeredEarlier(answer))
    await controller.answer(allow: true)
    controller.send(.close)
    #expect(Self.answers(commands).isEmpty)
    #expect(closes.count == 1)
  }

  @Test("a request already past its time shows expired at once")
  func pastItsTime() async {
    let commands = ScriptedCommands(["lifecycle request": Self.request(seconds: -1)])
    let controller = Self.controller(commands, closes: Closes(), clock: ManualClock())
    await controller.start()
    #expect(controller.phase == .expired)
  }

  @Test("a request the command line refuses to read says why, with Close alone")
  func aRefusedReadSaysWhy() async {
    let commands = ScriptedCommands(["lifecycle request": #"{"ok": false, "error": "No such request."}"#])
    let controller = Self.controller(commands, closes: Closes(), clock: ManualClock())
    await controller.start()
    #expect(controller.phase == .failed("No such request."))
    #expect(controller.state.card.buttons.map(\.id) == [.close])
  }

  @Test("an answer the command line did not take says why and never closes by itself")
  func aRefusedAnswerSaysWhy() async {
    let commands = ScriptedCommands([
      "lifecycle request": Self.request(),
      "lifecycle answer": #"{"ok": false, "error": "This request was answered already."}"#,
    ])
    let closes = Closes()
    let controller = Self.controller(commands, closes: closes, clock: ManualClock())
    await controller.start()
    await controller.answer(allow: true)
    #expect(controller.phase == .failed("This request was answered already."))
    #expect(closes.count == 0)
  }
}
