import AtlasCommands
import AtlasOnboarding
import Foundation

/// The choice of where agents work, and the VM's add, which runs as a command that outlives the window: its progress
/// is read back from the facts, so a window opened again finds it where it is.
extension OnboardingController {
  /// Continue on the choice: Second Login's account choice comes next, or the VM's add begins.
  func continueDesktop() {
    guard let kind = snapshot.shownDesktop else { return }
    snapshot.choices.desktop = kind
    if kind == .vm { addVM() }
  }

  /// Opened from Settings' Add a Desktop…: the window chooses a desktop to add beside those set up, and follows that
  /// add.
  func addDesktop() {
    snapshot.choices.adding = true
    snapshot.choices.started = true
    refresh()
  }

  /// Asks for the VM's add, or for the add under way to go on, named as the facts name it: the person allows it in the
  /// approval window. Never a second one while this window's runs or the facts say one runs, and none once the window
  /// has closed. Only the wait ends with the window; the add goes on.
  func addVM() {
    guard !closed, adding == nil, snapshot.vmAdd != .running else { return }
    snapshot.addingHere = true
    snapshot.addFailure = nil
    let name = snapshot.vm?.name
    let action = Action.addVM(name: name?.isEmpty == false ? name : nil)
    let folder = dependencies.commandsFolder
    adding = Task {
      let outcome = await commands.runLasting(action, keeping: folder)
      adding = nil
      guard !closed, !Task.isCancelled else { return }
      snapshot.addingHere = false
      if !outcome.ok { snapshot.addFailure = outcome.message }
      // Once added, the VM is no longer one being added: the window follows it by the name the add gave it.
      if outcome.ok, snapshot.choices.adding {
        snapshot.choices.added = outcome.decode(DesktopAdded.self)?.name ?? name ?? snapshot.vm?.name
      }
      refresh()
    }
    refresh()
  }
}
