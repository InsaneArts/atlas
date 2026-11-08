import AtlasCommands
import AtlasService

/// The Helper pane's "Your files": the home folder is read when the pane opens and on Check Again; Make Private runs at
/// once and Undo… asks first; letting agents copy sign-ins in asks first, stopping them runs at once.
extension SettingsController {
  /// Reads how the home folder stands for the helper, and whether agents may copy sign-ins in. A reading a change
  /// overtook is dropped, as the change's own answer is newer; one that failed leaves both unknown, saying why.
  func readFiles() async {
    let asked = files.generation
    let outcome = await dependencies.commands.run(.homeFiles)
    guard asked == files.generation else { return }
    if let home = HomeFiles.of(outcome) {
      keep(home)
    } else {
      files.home = nil
      files.signInCopies = nil
      files.problem = Words.sentence(outcome.message)
    }
  }

  /// Makes the home folder private from the helper, or opens it again once the person said Let It Open. A failure
  /// keeps what was known, saying why.
  ///
  /// - Parameter makePrivate: Whether to make it private.
  func changeHome(makePrivate: Bool) async {
    guard begin(home: true) else { return }
    let outcome = await dependencies.commands.run(makePrivate ? .makeHomePrivate : .openHome)
    end()
    if let home = HomeFiles.of(outcome) {
      keep(home)
    } else {
      files.problem = Words.sentence(outcome.message)
    }
  }

  /// Lets agents copy sign-ins in, once the person said Allow, or stops them. A failure keeps what was known, saying
  /// why.
  ///
  /// - Parameter allow: Whether agents may copy sign-ins in.
  func setSignInCopies(_ allow: Bool) async {
    guard begin(home: false) else { return }
    let outcome = await dependencies.commands.run(.copySignIns(allow))
    end()
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

  /// Starts a change, one at a time, overtaking any reading under way.
  ///
  /// - Parameter home: Whether it changes the home folder, which its button shows.
  /// - Returns: Whether it started; not while another runs.
  private func begin(home: Bool) -> Bool {
    guard !files.changing else { return false }
    files.changing = true
    files.changingHome = home
    files.generation += 1
    return true
  }

  /// Ends a change, overtaking any reading that started while it ran.
  private func end() {
    files.changing = false
    files.changingHome = false
    files.generation += 1
  }

  /// Keeps what a files command said of the home folder.
  ///
  /// - Parameter home: The home folder as it said.
  private func keep(_ home: HomeFiles) {
    files.home = home
    files.signInCopies = home.signInCopies
    files.problem = nil
  }
}
