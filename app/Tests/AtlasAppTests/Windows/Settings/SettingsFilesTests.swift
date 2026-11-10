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
    #expect(controller.state.privacy == nil, "nothing shows before Atlas has looked")

    await controller.readFiles()
    let open = controller.state.privacy?.home
    #expect(open?.title == "The helper can open 6 of your folders")
    #expect(open?.detail == "Such as dev and .config")
    #expect(open?.button == "Make Private")
    #expect(controller.state.privacy?.signInCopies == false)

    commands.answer("onboarding files", Self.privateHome)
    await controller.changeHome(makePrivate: true)
    let made = controller.state.privacy?.home
    #expect(made?.isPrivate == true)
    #expect(made?.title == "Your home folder is private from the helper")
    #expect(made?.detail == "Agents still open your builds: Atlas copies them in first")
    #expect(made?.button == "Undo…")
    #expect(commands.ran.contains(Action.makeHomePrivate.arguments))
  }

  @Test("a change that failed reads the home folder again, showing how it really stands, and keeps saying why")
  func aFailedChangeReadsAgain() async {
    let failure =
      #"{"ok": false, "code": "io", "error": "Atlas couldn't open your home folder again. In Terminal, run: chmod -a"}"#
    let commands = ScriptedCommands(["onboarding files": Self.privateHome])
    commands.whileRunning { arguments in
      // Undo took, then the look proving it failed; the next reading finds it open.
      if arguments == Action.openHome.arguments { commands.answer("onboarding files", failure) }
      if arguments == Action.homeFiles.arguments, commands.ran.contains(Action.openHome.arguments) {
        commands.answer("onboarding files", Self.open)
      }
    }
    let controller = SettingsController.scripted(commands: commands)
    await controller.readFiles()
    #expect(controller.state.privacy?.home?.isPrivate == true)
    await controller.changeHome(makePrivate: false)
    let files = controller.state.privacy
    #expect(files?.home?.isPrivate == false, "never still private once Undo may have taken")
    #expect(files?.status?.kind == .problem)
    #expect(files?.status?.text.contains("chmod -a") == true)
    #expect(commands.ran.filter { $0 == Action.homeFiles.arguments }.count == 2)
  }

  @Test("a change that failed and can't be read again leaves the home folder unknown, saying why")
  func aFailedChangeUnreadIsUnknown() async {
    let commands = ScriptedCommands(["onboarding files": Self.open])
    let controller = SettingsController.scripted(commands: commands)
    await controller.readFiles()
    commands.answer("onboarding files", #"{"ok": false, "code": "io", "error": "Atlas couldn't"}"#)
    await controller.changeHome(makePrivate: true)
    let files = controller.state.privacy
    #expect(files?.home == nil)
    #expect(files?.status == SettingsNote(text: "Atlas couldn't.", kind: .problem))
  }

  @Test("only the newest reading counts: an older one that ends later never hides what the newer found")
  func onlyTheNewestReadingCounts() async throws {
    let commands = ScriptedCommands(["onboarding files": Self.open])
    let gate = ReleaseSignal()
    var first = true
    commands.whileRunning { arguments in
      guard arguments == Action.homeFiles.arguments, first else { return }
      first = false
      await gate.wait()
      commands.answer("onboarding files", #"{"ok": false, "code": "io", "error": "Atlas couldn't"}"#)
    }
    let controller = SettingsController.scripted(commands: commands)
    let older = Task { await controller.readFiles() }
    try await settle("the older reading never started") { commands.ran.contains(Action.homeFiles.arguments) }
    await controller.readFiles()
    #expect(controller.state.privacy?.home?.isPrivate == false)
    gate.open()
    await older.value
    #expect(controller.state.privacy?.home?.isPrivate == false, "the older reading failed")
    #expect(controller.state.privacy?.status == nil)
  }

  @Test("a reading a change overtook is dropped, so an older answer never undoes the change's")
  func aStaleReadingIsDropped() async throws {
    let commands = ScriptedCommands(["onboarding files": Self.privateHome])
    let gate = ReleaseSignal()
    commands.whileRunning { arguments in
      if arguments == Action.homeFiles.arguments {
        await gate.wait()
        commands.answer("onboarding files", Self.privateHome)
      } else if arguments == Action.openHome.arguments {
        commands.answer("onboarding files", Self.open)
      }
    }
    let controller = SettingsController.scripted(commands: commands)
    let reading = Task { await controller.readFiles() }
    try await settle("the reading never started") { commands.ran.contains(Action.homeFiles.arguments) }
    await controller.changeHome(makePrivate: false)
    #expect(controller.state.privacy?.home?.isPrivate == false)
    gate.open()
    await reading.value
    #expect(controller.state.privacy?.home?.isPrivate == false, "the older reading said private")
  }

  @Test("a reading that failed leaves the home folder and the switch unknown, saying why under the section")
  func aFailedReadingIsUnknown() async {
    let commands = ScriptedCommands(["onboarding files": Self.privateHome])
    let controller = SettingsController.scripted(commands: commands)
    await controller.readFiles()
    #expect(controller.state.privacy?.home != nil)
    let missing = #"{"ok": false, "code": "identity", "error": "The helper account was not found"}"#
    commands.answer("onboarding files", missing)
    await controller.readFiles()
    let files = controller.state.privacy
    #expect(files?.home == nil)
    #expect(files?.signInCopies == nil)
    #expect(files?.status == SettingsNote(text: "The helper account was not found.", kind: .problem))
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
    #expect(controller.state.privacy?.home?.isPrivate == false)
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
    #expect(controller.state.privacy?.signInCopies == true)

    commands.answer("onboarding sign-ins", #"{"ok": true, "signInCopies": false}"#)
    controller.send(.setSignInCopies(false))
    try await settle("turning copies off never ran") { commands.ran.contains(Action.copySignIns(false).arguments) }
    #expect(controller.state.alert == nil)
    try await settle("the switch never turned off") { controller.state.privacy?.signInCopies == false }
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
      let commands = ScriptedCommands([
        "onboarding status": OnboardingReplies.finishedMac, "onboarding files": Self.open,
      ])
      await HomeOffer.offer(commands: commands) { _ in makesPrivate }
      #expect(commands.ran.contains(Action.makeHomePrivate.arguments) == makesPrivate)
      #expect(commands.ran.contains(Action.homeOffered.arguments) == !makesPrivate)
    }
  }

  @Test("the offer looks at the home folder only once setup is finished and it was never offered")
  func theOfferReadsTheFactsFirst() async {
    let finished = OnboardingReplies.finishedMac
    let offered = finished.replacingOccurrences(
      of: #""finished": true"#,
      with: #""finished": true, "homePrivacyOffered": true"#
    )
    for status in [OnboardingReplies.checkingMac, offered, #"{"ok": false, "error": "no"}"#] {
      let commands = ScriptedCommands(["onboarding status": status, "onboarding files": Self.open])
      var asked = false
      await HomeOffer.offer(commands: commands) { _ in
        asked = true
        return true
      }
      #expect(!asked)
      #expect(commands.ran == [Action.onboardingStatus.arguments], "the home folder is never read")
    }
  }

  @Test("a Make Private from the offer that failed says why once and settles the offer")
  func aFailedOfferSettlesIt() async {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "onboarding files": Self.open,
    ])
    commands.whileRunning { arguments in
      if arguments == Action.makeHomePrivate.arguments {
        commands.answer("onboarding files", #"{"ok": false, "code": "io", "error": "Atlas couldn't"}"#)
      }
    }
    var told: [String] = []
    await HomeOffer.offer(commands: commands, ask: { _ in true }, tell: { told.append($0) })
    #expect(told == ["Atlas couldn't."])
    #expect(commands.ran.contains(Action.homeOffered.arguments))
  }

  @Test("the offer stays quiet once the home folder is private")
  func theOfferStaysQuiet() async {
    let commands = ScriptedCommands([
      "onboarding status": OnboardingReplies.finishedMac, "onboarding files": Self.privateHome,
    ])
    var asked = false
    await HomeOffer.offer(commands: commands) { _ in
      asked = true
      return true
    }
    #expect(!asked)
    #expect(commands.ran == [Action.onboardingStatus.arguments, Action.homeFiles.arguments])
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
