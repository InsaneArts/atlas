import AppKit
import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("Approval shell", .serialized, AppKitState())
struct ApprovalShellTests {
  /// The request's ID.
  private static let id = String(repeating: "a", count: 48)

  /// A reply reading a request still to be answered.
  private static let request: String = {
    let until = Int64(Date().timeIntervalSince1970 * 1000) + 600_000
    let vm = String(repeating: "b", count: 24)
    return #"{"ok": true, "request": {"id": "\#(id)", "kind": "add", "name": "work", "vm": "\#(vm)", "#
      + #""settings": "~/.config/atlas.toml", "session": "a terminal", "until": \#(until)}}"#
  }()

  /// A controller over scripted commands that read the request and refuse it, shown asking.
  private static func asking() async -> (ScriptedCommands, ApprovalController) {
    let commands = ScriptedCommands([
      "lifecycle request": request,
      "lifecycle answer": #"{"ok": true, "answer": "refused"}"#,
    ])
    let controller = ApprovalController(
      id: id,
      commands: commands,
      links: ApprovalLinks(clock: ManualClock())
    )
    await controller.start()
    return (commands, controller)
  }

  /// The answers the scripted command line was asked to keep, as their last argument.
  private static func answers(_ commands: ScriptedCommands) -> [String] {
    commands.ran
      .filter { $0.prefix(2) == ["lifecycle", "answer"] }
      .compactMap(\.last)
  }

  @Test("the window's native Close refuses a request still asked, as Escape does")
  func nativeCloseRefuses() async throws {
    let (commands, controller) = await Self.asking()
    let window = ApprovalWindow(controller: controller)
    window.performClose(nil)
    try await settle("the native Close never refused") { controller.phase == .answered(allowed: false) }
    #expect(Self.answers(commands) == ["--refuse"])
  }

  @Test("Quit while a request is asked refuses it first and waits; once answered it quits at once")
  func quitRefusesFirst() async throws {
    let (commands, controller) = await Self.asking()
    let delegate = ApprovalAppDelegate(controller: controller)
    #expect(delegate.applicationShouldTerminate(NSApplication.shared) == .terminateCancel)
    try await settle("Quit never refused") { controller.phase == .answered(allowed: false) }
    #expect(delegate.applicationShouldTerminate(NSApplication.shared) == .terminateNow)
    #expect(Self.answers(commands) == ["--refuse"], "quitting again answers nothing more")
  }
}
