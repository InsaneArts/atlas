import AtlasCommands
import AtlasService

/// The Helper pane's "Your files": the home folder is read when the pane opens and on Check Again; Make Private runs at
/// once and Undo… asks first; letting agents copy sign-ins in asks first, stopping them runs at once.
extension SettingsController {
  /// Reads how the home folder stands for the helper, and whether agents may copy sign-ins in.
  func readFiles() async {
    let outcome = await dependencies.commands.run(.homeFiles)
    take(outcome)
  }

  /// Makes the home folder private from the helper, or opens it again once the person said Let It Open.
  ///
  /// - Parameter makePrivate: Whether to make it private.
  func changeHome(makePrivate: Bool) async {
    guard !files.changing else { return }
    files.changing = true
    let outcome = await dependencies.commands.run(makePrivate ? .makeHomePrivate : .openHome)
    files.changing = false
    take(outcome)
  }

  /// Lets agents copy sign-ins in, once the person said Allow, or stops them.
  ///
  /// - Parameter allow: Whether agents may copy sign-ins in.
  func setSignInCopies(_ allow: Bool) async {
    guard !files.changing else { return }
    files.changing = true
    let outcome = await dependencies.commands.run(.copySignIns(allow))
    files.changing = false
    if let allowed = SignInCopies.of(outcome) {
      files.signInCopies = allowed
      files.problem = nil
    } else {
      files.problem = Words.sentence(outcome.message)
    }
  }

  /// The person moved the sign-ins switch: turning it on asks first, turning it off runs at once.
  ///
  /// - Parameter allow: Where the switch went.
  func askSignInCopies(_ allow: Bool) {
    if allow {
      ask(.allowSignIns)
    } else {
      // Unstructured, as `send` is the view's synchronous callback.
      Task { await setSignInCopies(false) }
    }
  }

  /// Keeps what a files command said, or its failure.
  ///
  /// - Parameter outcome: The command's outcome.
  private func take(_ outcome: Outcome) {
    if let home = HomeFiles.of(outcome) {
      files.home = home
      files.signInCopies = home.signInCopies
      files.problem = nil
    } else {
      files.problem = Words.sentence(outcome.message)
    }
  }
}
