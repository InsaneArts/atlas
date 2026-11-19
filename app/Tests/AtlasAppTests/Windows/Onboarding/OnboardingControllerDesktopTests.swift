import AsyncSupportTesting
import AtlasCommands
import AtlasCommandsTesting
import AtlasOnboarding
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

/// A fresh Mac that can make a VM.
private let freshWithRoom = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": false, "candidates": [], "desktops": {"vm": {"available": true}}}
  """

/// The VM `vm` being added, its download under way and its add's process `running`.
private func downloading(running: Bool) -> String {
  """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": true, "candidates": [], "desktops": {"vm": {"available": true}},
   "vm": {"name": "vm", "kind": "vm", "listed": true, "lifecycle": "adding", "steps": ["agent"],
          "progress": {"phase": "downloading", "received": 1000000000, "total": 25600000000},
          "adder": {"running": \(running)}, "ready": false}}
  """
}

/// The VM `vm` added and ready for agents.
private let added = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": true, "finished": true, "candidates": [], "desktops": {"vm": {"available": true}},
   "vm": {"name": "vm", "kind": "vm", "listed": true, "lifecycle": "created", "steps": [], "ready": true}}
  """

/// Second Login set up and finished, on a Mac that can make a VM, as Settings' Add a Desktop… opens the window.
private let setUp = """
  {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
   "configured": true, "finished": true, "candidates": [], "desktops": {"vm": {"available": true}},
   "helper": {"exists": true, "uid": 502, "name": "atlas", "paired": true, "answering": true, "hidden": true,
              "permissions": {"screenCapture": true, "accessibility": true}}}
  """

@Suite("OnboardingController desktop")
struct OnboardingControllerDesktopTests {
  @Test("Add a Desktop opens the choice with Second Login added and the VM picked, then checks the VM its add made")
  func addADesktopFromSettings() async throws {
    let commands = ScriptedCommands([
      "onboarding status": setUp,
      "desktop add": #"{"ok": true, "added": {"name": "vm-2", "kind": "vm"}, "ready": true}"#,
      "onboarding finish": #"{"ok": true, "ready": true, "desktop": "vm-2"}"#,
    ])
    let clock = ManualClock()
    let controller = OnboardingController.scripted(commands, clock: clock)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.addDesktop()
      #expect(controller.snapshot.step == .chooseDesktop, "never Second Login's own steps")
      #expect(controller.snapshot.shownDesktop == .vm)
      #expect(controller.state.footer.back == nil, "no welcome to go back to")
      controller.send(.continueDesktop)
      try await settle("the add never ended") { controller.snapshot.choices.added == "vm-2" }
      #expect(commands.ran.contains(Action.addVM(name: nil).arguments))
      #expect(controller.snapshot.step == .finishing)
      try await controller.untilSettled(commands, clock: clock)
      try await settle("the check never passed") { controller.snapshot.finish == .passed }
      #expect(commands.ran.contains(Action.finishVM("vm-2").arguments))
    }
  }

  @Test("Second Login is picked when the choice opens, and Continue goes to today's account choice")
  func secondLoginIsPickedAsTheChoiceOpens() async throws {
    let commands = ScriptedCommands(["onboarding status": freshWithRoom])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.begin)
      #expect(controller.snapshot.step == .chooseDesktop)
      #expect(controller.state.footer.next?.isEnabled == true && controller.state.footer.next?.isDefault == true)
      controller.send(.continueDesktop)
      #expect(controller.snapshot.step == .chooseHelper)
      controller.send(.back)
      #expect(controller.snapshot.step == .chooseDesktop, "Back on the account choice comes back here")
      controller.send(.back)
      #expect(controller.snapshot.step == .welcome)
      #expect(!commands.ran.contains { $0.first == "desktop" }, "nothing is added for Second Login")
    }
  }

  @Test("Continue with VM picked asks for the add, which outlives the window, and lands on the download")
  func continueWithAVMAsksForTheAdd() async throws {
    let commands = ScriptedCommands(["onboarding status": freshWithRoom, "desktop add": downloading(running: true)])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.begin)
      controller.send(.pickDesktop(.vm))
      #expect(controller.state.footer.track?.count == 8, "picking VM makes the track eight")
      controller.send(.continueDesktop)
      #expect(controller.snapshot.step == .downloadVM)
      try await settle("the add never ran") { commands.ran.contains(Action.addVM(name: nil).arguments) }
      commands.answer("onboarding status", downloading(running: true))
      controller.refresh()
      try await settle("the download never showed from the facts") { controller.snapshot.facts.vm != nil }
      #expect(controller.snapshot.step == .downloadVM)
    }
  }

  @Test("a VM picked that this Mac cannot run is never chosen: Continue goes on with Second Login")
  func aVMThisMacCannotRunIsNeverChosen() async throws {
    let full = freshWithRoom.replacingOccurrences(
      of: #"{"available": true}"#,
      with: #"{"available": false, "why": "disk"}"#
    )
    let controller = OnboardingController.scripted(ScriptedCommands(["onboarding status": full]))
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.send(.begin)
      controller.send(.pickDesktop(.vm))
      controller.send(.continueDesktop)
      #expect(controller.snapshot.choices.desktop == .secondLogin)
      #expect(controller.snapshot.step == .chooseHelper)
    }
  }

  @Test("an add whose process ended offers Go On, which asks the same VM's add again")
  func anEndedAddOffersGoOn() async throws {
    let commands = ScriptedCommands([
      "onboarding status": downloading(running: false), "desktop add": downloading(running: true),
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      #expect(controller.snapshot.step == .downloadVM, "a window reopened mid-download lands on the download")
      #expect(controller.state.footer.next?.title == .goOn)
      controller.send(.goOnAdding)
      try await settle("Go On never asked") { commands.ran.contains(Action.addVM(name: "vm").arguments) }
    }
  }

  @Test("the VM's check runs on the VM, and Done keeps no home folder private")
  func theVMsCheckAndDone() async throws {
    let commands = ScriptedCommands([
      "onboarding status": added, "onboarding finish": #"{"ok": true, "ready": true}"#,
      "onboarding login": #"{"ok": true}"#, "menubar": #"{"ok": true}"#,
    ])
    let clock = ManualClock()
    let controller = OnboardingController.scripted(commands, clock: clock)
    try await controller.whileOpen { _ in
      try await controller.untilSettled(commands, clock: clock)
      try await settle("the check never passed") { controller.snapshot.finish == .passed }
      #expect(commands.ran.contains(Action.finishVM("vm").arguments))
      #expect(!commands.ran.contains(Action.finishSetup.arguments))
      controller.send(.continueFinishing)
      try await settle("the agents never came") { controller.snapshot.step == .agents }
      await controller.complete()
      #expect(!commands.ran.contains { $0.prefix(2) == ["onboarding", "files"] })
      #expect(controller.closeRequested)
    }
  }

  @Test("adding a desktop from Settings leaves Open at Login as it was")
  func addingLeavesOpenAtLogin() async throws {
    let commands = ScriptedCommands([
      "onboarding status": setUp, "onboarding login": #"{"ok": true}"#, "menubar": #"{"ok": true}"#,
    ])
    let controller = OnboardingController.scripted(commands)
    try await controller.whileOpen { _ in
      try await controller.untilLoaded()
      controller.addDesktop()
      controller.snapshot.choices.added = "vm-2"
      controller.snapshot.opensAtLogin = true
      #expect(!OnboardingPresenter.done(controller.snapshot).offersOpenAtLogin, "Done offers no switch for it")
      await controller.complete()
      #expect(!commands.ran.contains { $0.prefix(2) == ["onboarding", "login"] })
      #expect(commands.ran.contains(Action.openMenuBar.arguments))
      #expect(controller.closeRequested)
    }
  }
}
