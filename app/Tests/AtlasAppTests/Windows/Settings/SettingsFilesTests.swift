import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings your files")
struct SettingsFilesTests {
  /// The home folder open to the helper, with six folders it can open.
  static let open =
    #"{"ok": true, "private": false, "readable": 6, "examples": ["dev", ".config"], "offered": false, "signInCopies": false}"#
  /// The home folder private from the helper.
  static let privateHome =
    #"{"ok": true, "private": true, "readable": 0, "examples": [], "offered": true, "signInCopies": false}"#

  @Test("the section says how many folders the helper can open and offers Make Private, then Undo… once private")
  func wordsFollowTheHomeFolder() async {
    let commands = ScriptedCommands(["onboarding files": Self.open])
    let controller = SettingsController.scripted(commands: commands)
    #expect(controller.state.helper.files == nil, "nothing shows before Atlas has looked")

    await controller.readFiles()
    let open = controller.state.helper.files?.home
    #expect(open?.title == "The helper can open 6 of your folders")
    #expect(open?.detail == "Such as dev and .config")
    #expect(open?.button == "Make Private")
    #expect(controller.state.helper.files?.signInCopies == false)

    commands.answer("onboarding files", Self.privateHome)
    await controller.changeHome(makePrivate: true)
    let made = controller.state.helper.files?.home
    #expect(made?.isPrivate == true)
    #expect(made?.title == "Your home folder is private from the helper")
    #expect(made?.detail == "Agents still open your builds: Atlas copies them in first")
    #expect(made?.button == "Undo…")
    #expect(commands.ran.contains(Action.makeHomePrivate.arguments))
  }

  @Test("a change that failed says why under the section, keeping the row as it was")
  func aFailedChangeSaysWhy() async {
    let commands = ScriptedCommands(["onboarding files": Self.open])
    let controller = SettingsController.scripted(commands: commands)
    await controller.readFiles()
    commands.answer(
      "onboarding files",
      #"{"ok": false, "code": "io", "error": "Atlas couldn't make your home folder private. In Terminal, run: chmod +a# 0 \"user:atlas deny list,search\" ~"}"#
    )
    await controller.changeHome(makePrivate: true)
    let files = controller.state.helper.files
    #expect(files?.home?.isPrivate == false)
    #expect(files?.status?.kind == .problem)
    #expect(files?.status?.text.contains("chmod +a# 0") == true)
  }

  @Test("Undo… asks first, with Keep Private as the default, and runs only on Let It Open")
  func undoAsksFirst() async throws {
    let commands = ScriptedCommands(["onboarding files": Self.privateHome])
    let controller = SettingsController.scripted(commands: commands)
    await controller.readFiles()
    controller.send(.askOpenHome)
    let alert = try #require(controller.state.alert)
    #expect(alert.title == "Let the helper open your home folder again?")
    let fallback = try #require(alert.buttons.first { $0.role == .safeCancel })
    #expect(fallback.title == "Keep Private")
    #expect(alert.buttons.contains { $0.title == "Let It Open" && $0.role == .destructive })
    #expect(!commands.ran.contains(Action.openHome.arguments))

    commands.answer("onboarding files", Self.open)
    await controller.confirmAlert()
    #expect(commands.ran.contains(Action.openHome.arguments))
    #expect(controller.state.helper.files?.home?.isPrivate == false)
  }

  @Test("letting agents copy sign-ins in asks first with Don't Allow as the default; stopping them doesn't ask")
  func signInCopiesAskOnlyToTurnOn() async throws {
    let commands = ScriptedCommands([
      "onboarding files": Self.privateHome, "onboarding sign-ins": #"{"ok": true, "signInCopies": true}"#,
    ])
    let controller = SettingsController.scripted(commands: commands)
    await controller.readFiles()
    controller.send(.setSignInCopies(true))
    let alert = try #require(controller.state.alert)
    #expect(alert.buttons.first { $0.role == .safeCancel }?.title == "Don't Allow")
    #expect(!commands.ran.contains(Action.copySignIns(true).arguments))
    await controller.confirmAlert()
    #expect(commands.ran.contains(Action.copySignIns(true).arguments))
    #expect(controller.state.helper.files?.signInCopies == true)

    commands.answer("onboarding sign-ins", #"{"ok": true, "signInCopies": false}"#)
    controller.send(.setSignInCopies(false))
    try await settle("turning copies off never ran") { commands.ran.contains(Action.copySignIns(false).arguments) }
    #expect(controller.state.alert == nil)
    try await settle("the switch never turned off") { controller.state.helper.files?.signInCopies == false }
  }

  @Test("the one-time offer asks only while the home folder is open and was never offered")
  func theOfferAsksOnce() {
    let files = { (isPrivate: Bool, offered: Bool) in
      HomeFiles(isPrivate: isPrivate, readable: 6, examples: ["dev"], offered: offered, signInCopies: false)
    }
    #expect(HomeOffer.offers(files(false, false)))
    #expect(!HomeOffer.offers(files(true, false)))
    #expect(!HomeOffer.offers(files(false, true)))
  }

  @Test("the offer's Make Private makes it private; Not Now only remembers it was offered")
  func theOfferDoesWhatThePersonChose() async {
    for makesPrivate in [true, false] {
      let commands = ScriptedCommands(["onboarding files": Self.open])
      await HomeOffer.offer(commands: commands) { _ in makesPrivate }
      #expect(commands.ran.contains(Action.makeHomePrivate.arguments) == makesPrivate)
      #expect(commands.ran.contains(Action.homeOffered.arguments) == !makesPrivate)
    }
  }

  @Test("the offer stays quiet once the home folder is private")
  func theOfferStaysQuiet() async {
    let commands = ScriptedCommands(["onboarding files": Self.privateHome])
    var asked = false
    await HomeOffer.offer(commands: commands) { _ in
      asked = true
      return true
    }
    #expect(!asked)
    #expect(commands.ran == [Action.homeFiles.arguments])
  }

  @Test("the offer says what the helper can open and that builds still open")
  func theOfferSaysWhatItMeans() {
    let files = HomeFiles(
      isPrivate: false,
      readable: 6,
      examples: ["dev", ".config"],
      offered: false,
      signInCopies: false
    )
    let said = "The helper can open 6 of your folders, such as dev and .config."
    let builds = "Agents still open your builds: Atlas copies them in first."
    #expect(SettingsWords.homeOfferMessage(files) == "\(said) \(builds)")
  }
}
