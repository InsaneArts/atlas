import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasOnboarding
import AtlasService
import AtlasUI
import CoreGraphics
import Foundation
import Testing

@testable import AtlasApp

@Suite("OnboardingController helper screens and first reading")
struct OnboardingControllerHelperScreensTests {
  @Test("Not Now on the helper's allow screen closes the window and allows nothing")
  func notNowClosesTheWindow() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.restarting, "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(controller.snapshot.step == .allow)
      controller.send(.notNow)
      #expect(controller.closeRequested)
      #expect(!commands.ran.contains { $0.prefix(2) == ["onboarding", "accept"] }, "nothing was allowed")
    }
  }

  @Test("a permission this window saw on and that is off again reads Turned off")
  func aPermissionSeenOnThenOffIsTurnedOff() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.helper(accessibility: true), "onboarding window": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      commands.answer("onboarding status", OnboardingReplies.helper(accessibility: false))
      controller.refresh()
      try await settle("Accessibility never read as off") { !controller.snapshot.facts.asker.accessibility }
      guard case .screen(.permissions(let screen)) = controller.state.content else {
        Issue.record("the permissions screen is not shown")
        return
      }
      #expect(screen.accessibility.standing == .turnedOff("Turned off"))
    }
  }

  @Test("the asking account's picture is read once, by its short name, for the allow screen")
  func theAskersPictureIsReadOnce() async throws {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.restarting, "onboarding window": #"{"ok": true}"#,
    ])
    let pixel = try #require(
      CGContext(
        data: nil,
        width: 1,
        height: 1,
        bitsPerComponent: 8,
        bytesPerRow: 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      )?
      .makeImage()
    )
    var asked: [String] = []
    var dependencies = OnboardingController.scripted(commands).dependencies
    dependencies.accountPicture = { name in
      asked.append(name)
      return IconImage(id: name, image: pixel)
    }
    let controller = OnboardingController(dependencies: dependencies)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.refresh()
      try await settle("the facts were never read again") { commands.ran.filter { $0.first == "onboarding" }.count > 2 }
      #expect(asked == ["me"])
      guard case .screen(.allow(let allow)) = controller.state.content else {
        Issue.record("the allow screen is not shown")
        return
      }
      #expect(allow.picture?.id == "me")
    }
  }

  @Test("a first reading that could not run Atlas's command line at all says the files look incomplete")
  func aCommandLineThatCannotRunSaysSo() async throws {
    let controller = OnboardingController.scripted(ScriptedCommands([:]))
    try await controller.whileOpen { _ in
      try await settle("the first reading never failed") { controller.snapshot.problem != nil }
      #expect(controller.snapshot.problem == "Atlas’s files look incomplete. Reinstall Atlas, then open it again.")
    }
    let refused = Outcome(ok: false, reply: Data(#"{"ok": false}"#.utf8), message: "the status could not be read")
    #expect(OnboardingController.firstReadingProblem(refused) == "the status could not be read.")
  }

  @Test("Show in Finder shows Atlas.app selected in the Finder")
  func showInFinderRevealsTheApp() {
    let opened = OpenedURLs()
    let controller = OnboardingController.scripted(ScriptedCommands([:]), opened: opened)
    controller.send(.showInFinder)
    #expect(opened.urls == [controller.dependencies.app])
  }
}
