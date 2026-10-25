import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasOnboarding
import Testing

@testable import AtlasApp

@Suite("OnboardingController queue")
struct OnboardingControllerQueueTests {
  @Test(
    "Allow keeps the approved account and coalesces repeated clicks before and during execution",
    arguments: [false, true]
  )
  func allowKeepsItsApprovedAccount(changedBeforeTaking: Bool) async throws {
    let gate = ReleaseSignal()
    defer { gate.open() }
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false),
      "onboarding window": #"{"ok": true}"#,
      "onboarding accept": #"{"ok": true}"#,
    ])
    commands.whileRunning { arguments in
      if arguments.starts(with: ["onboarding", "accept"]) { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.snapshot.facts.asker.uid = 501
      controller.snapshot.facts.asker.name = "approved"
      controller.send(.allow)
      controller.send(.allow)
      if changedBeforeTaking {
        controller.snapshot.facts.asker.uid = 502
        controller.snapshot.facts.asker.name = "later"
      }
      controller.send(.allow)
      let work = try await controller.untilTaken {
        controller.snapshot.facts.asker.uid = 503
        controller.snapshot.facts.asker.name = "newest"
        controller.send(.allow)
      }
      try await settle("Allow never started") {
        commands.ran.contains { $0.starts(with: ["onboarding", "accept"]) }
      }
      controller.send(.allow)
      #expect(controller.pending.isEmpty, "another Allow must not wait behind the approved one")
      gate.open()
      await work.value
      #expect(
        commands.ran.filter { $0.starts(with: ["onboarding", "accept"]) }
          == [Action.allow(uid: 501, name: "approved").arguments]
      )
    }
  }

  @Test("closing after Allow is taken but before it starts approves no account")
  func closingBeforeAllowStartsApprovesNothing() async throws {
    guard #available(macOS 26, *) else { return }
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false),
      "onboarding window": #"{"ok": true}"#,
      "onboarding accept": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.allow)
      var closing: Task<Void, Never>?
      let work = try await controller.untilTaken {
        closing = Task.immediate { await controller.close() }
      }
      await work.value
      await closing?.value
      #expect(!commands.ran.contains { $0.starts(with: ["onboarding", "accept"]) })
    }
  }

  @Test("Back between the queue taking Lock and Switch and its start keeps it from locking the screen")
  func backBeforeARequestStartsStopsIt() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.switchingMac, "switch": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.lockAndSwitch)
      let work = try await controller.untilTaken { controller.send(.back) }
      await work.value
      #expect(!commands.ran.contains(Action.switchTo("atlas").arguments), "the screen locked after Back")
      #expect(controller.snapshot.working == nil)
    }
  }

  @Test("closing the window between the queue taking a permission's page and its start keeps the page shut")
  func closingBeforeARequestStartsStopsIt() async throws {
    // The close starts within the queue's turn, as only a task that runs at once until its first wait can.
    guard #available(macOS 26, *) else { return }
    let opened = OpenedURLs()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands, opened: opened)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.openSettings(id: Permission.accessibility.id))
      var closing: Task<Void, Never>?
      let work = try await controller.untilTaken {
        closing = Task.immediate { await controller.close() }
      }
      await work.value
      await closing?.value
      #expect(opened.urls.isEmpty, "System Settings opened after the window closed")
      #expect(controller.snapshot.guiding == nil)
    }
  }

  @Test("a page Back dropped and asked for again opens after the pages asked for in between")
  func aRequestAskedForAgainTakesItsNewPlace() async throws {
    let opened = OpenedURLs()
    let gate = ReleaseSignal()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false), "onboarding window": #"{"ok": true}"#,
      "onboarding accept": #"{"ok": true}"#,
    ])
    commands.whileRunning { arguments in
      if arguments.prefix(2) == ["onboarding", "accept"] { await gate.wait() }
    }
    let controller = OnboardingController.scripted(commands, opened: opened)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      // Allow holds the queue while the pages are asked for behind it.
      controller.send(.allow)
      try await settle("Allow never started") { controller.snapshot.working != nil }
      controller.send(.openSettings(id: Permission.accessibility.id))
      controller.send(.closePanel)
      controller.send(.openSettings(id: Permission.screenRecording.id))
      controller.send(.openSettings(id: Permission.accessibility.id))
      gate.open()
      try await settle("both pages never opened") { opened.urls.count == 2 }
      #expect(opened.urls == [Permission.screenRecording, .accessibility].compactMap(\.settingsURL))
      #expect(controller.snapshot.guiding == .accessibility)
    }
  }
}
