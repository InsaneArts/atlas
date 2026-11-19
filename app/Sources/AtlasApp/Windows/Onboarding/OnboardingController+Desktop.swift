import AtlasCommands
import AtlasOnboarding
import Foundation

/// The choice of where agents work, and the VM's add, which runs as a command that outlives the window: its progress
/// is read back from the facts, so a window opened again finds it where it is.
extension OnboardingController {
  /// Continue on the choice: Second Login's account choice comes next, or the VM's add begins.
  func continueDesktop() {
    let kind = snapshot.shownDesktop
    snapshot.choices.desktop = kind
    if kind == .vm { addVM() }
  }

  /// Asks for the VM's add, or for the add under way to go on, named as the facts name it: the person allows it in the
  /// approval window. Never a second one while this window's runs or the facts say one runs, and none once the window
  /// has closed. Only the wait ends with the window; the add goes on.
  func addVM() {
    guard !closed, adding == nil, snapshot.vmAdd != .running else { return }
    snapshot.addingHere = true
    snapshot.addFailure = nil
    let name = snapshot.facts.vm?.name
    let action = Action.addVM(name: name?.isEmpty == false ? name : nil)
    let folder = dependencies.commandsFolder
    adding = Task {
      let outcome = await commands.runLasting(action, keeping: folder)
      adding = nil
      guard !closed, !Task.isCancelled else { return }
      snapshot.addingHere = false
      if !outcome.ok { snapshot.addFailure = outcome.message }
      refresh()
    }
    refresh()
  }
}
