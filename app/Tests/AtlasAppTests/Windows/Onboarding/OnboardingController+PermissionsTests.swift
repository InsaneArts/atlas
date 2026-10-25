import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasOnboarding
import Testing

@testable import AtlasApp

@Suite("OnboardingController permissions")
struct OnboardingControllerPermissionsTests {
  @Test("the helper allows exactly the account it showed")
  func theHelperAllowsTheAccountShown() async throws {
    let commands = ScriptedCommands([
      "onboarding status": """
      {"ok": true, "role": "helper", "me": {"name": "atlas"}, "app": {"installed": true, "signed": true},
       "helper": {"controller": {"uid": 501, "name": "me"}, "installed": false, "running": false}}
      """,
      "onboarding accept": #"{"ok": true}"#,
      "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(controller.snapshot.step == .allow)
      controller.send(.allow)
      let work = try await controller.untilTaken {}
      await work.value
      #expect(commands.ran.contains(Action.allow(uid: 501, name: "me").arguments))
    }
  }

  @Test("a permission's button opens its own Privacy & Security page, with the panel beside it")
  func allowingAPermissionOpensItsPage() async throws {
    let opened = OpenedURLs()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands, opened: opened)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.openSettings(id: Permission.screenRecording.id))
      try await settle("the page never opened") {
        opened.urls == [Permission.screenRecording.settingsURL].compactMap { $0 }
      }
      #expect(controller.snapshot.guiding == .screenRecording)
    }
  }

  @Test("the panel opens the next permission's page once its switch is on, and steps aside once both are")
  func thePanelMovesOn() async throws {
    let opened = OpenedURLs()
    let clock = ManualClock()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands, opened: opened, clock: clock)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.openSettings(id: Permission.accessibility.id))
      try await settle("the panel never opened") { controller.snapshot.guiding == .accessibility }
      controller.refresh()
      try await settle("the facts were never read again") {
        commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 2
      }
      #expect(controller.snapshot.guiding == .accessibility, "still waiting for the switch")
      commands.answer("onboarding status", OnboardingReplies.helper(accessibility: true))
      controller.refresh()
      try await clock.untilSleeping(for: OnboardingController.tickShown, "Accessibility's tick never showed")
      clock.advance(by: OnboardingController.tickShown)
      try await settle("the panel never moved on") { controller.snapshot.guiding == .screenRecording }
      #expect(opened.urls == [Permission.accessibility, .screenRecording].compactMap(\.settingsURL))
      commands.answer("onboarding status", OnboardingReplies.helper(accessibility: true, screenCapture: true))
      controller.refresh()
      try await clock.untilSleeping(for: OnboardingController.tickShown, "Screen Recording's tick never showed")
      clock.advance(by: OnboardingController.tickShown)
      try await settle("the panel never stepped aside") { controller.snapshot.guiding == nil }
      #expect(controller.snapshot.step == .helperDone)
      #expect(opened.urls.count == 2, "each page opens once")
    }
  }

  @Test(
    "a tick under way moves on once however often the facts are read, and Back stops it even if the panel opens again"
  )
  func aTickUnderWay() async throws {
    let opened = OpenedURLs()
    let clock = ManualClock()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands, opened: opened, clock: clock)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.openSettings(id: Permission.accessibility.id))
      try await settle("the panel never opened") { controller.snapshot.guiding == .accessibility }
      commands.answer("onboarding status", OnboardingReplies.helper(accessibility: true))
      controller.refresh()
      try await clock.untilSleeping(for: OnboardingController.tickShown, "the tick never started")
      controller.refresh()
      try await settle("the facts were never read again") {
        commands.ran.filter { $0 == Action.onboardingStatus.arguments }.count == 3
      }
      // The reading's own pause, once it has begun, says the third reading has moved the panel on all it will.
      try await clock.untilSleeping(for: OnboardingController.readEvery, "the third reading never paused")
      #expect(clock.waiting.sorted() == [OnboardingController.tickShown, OnboardingController.readEvery])
      #expect(controller.pending.isEmpty, "one tick however often the facts are read: no second one waits")
      let tick = try #require(controller.performing?.work, "the tick is under way")
      controller.send(.closePanel)
      controller.send(.openSettings(id: Permission.accessibility.id))
      #expect(clock.waiting == [OnboardingController.readEvery], "Back ended the tick's wait")
      // The tick's own end is the sentinel: once it has ended, it has done all it ever will.
      try await tick.value("the stopped tick never ended")
      let accessibility = try #require(Permission.accessibility.settingsURL)
      try await settle("Screen Recording's page opened, or Accessibility's never again") {
        opened.urls == [accessibility, accessibility]
      }
      #expect(controller.snapshot.guiding == .accessibility)
    }
  }

  @Test("Back closes the panel and stops it moving on")
  func backClosesThePanel() async throws {
    let opened = OpenedURLs()
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: false), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands, opened: opened)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.openSettings(id: Permission.accessibility.id))
      controller.send(.closePanel)
      #expect(controller.snapshot.guiding == nil)
      #expect(controller.snapshot.step == .permissions)
      // A page asked for after Back is opened; the one Back dropped, queued ahead of it, never is.
      controller.send(.openSettings(id: Permission.screenRecording.id))
      try await settle("the next page never opened") { controller.snapshot.guiding == .screenRecording }
      #expect(opened.urls == [Permission.screenRecording.settingsURL].compactMap { $0 })
    }
  }
}
