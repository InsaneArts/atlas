import AtlasCommands
import AtlasService
import AtlasUI
import Foundation

/// The person's account side: the helper chosen, created or given up.
extension OnboardingController {
  /// Continue on the choice: the form for a new account, else set the picked account up as the helper.
  func useHelper() async {
    switch snapshot.shownPick {
    case .newAccount: snapshot.choices.creating = true
    case .existing(let name):
      let title = snapshot.facts.candidates.first { $0.name == name }?.title ?? name
      // The account Atlas was set up in before needs no approval.
      let words = snapshot.facts.onlyChoice ? OnboardingWords.choosing(title) : OnboardingWords.settingUp(title)
      noteReadied(await act(.chooseHelper(name), words))
    case nil: break
    }
  }

  /// Creates the account. The password reaches macOS on standard input, never in the arguments, and leaves the form
  /// as it is handed over.
  func createAccount() async {
    guard snapshot.form.canCreate, snapshot.working == nil else { return }
    let action = Action.createAccount(account: snapshot.form.accountName, fullName: snapshot.form.fullName)
    var secret = Data(snapshot.form.password.utf8)
    // One line: the command reads the password up to its newline.
    secret.append(0x0A)
    // Cleared from the window as soon as it is handed to macOS.
    snapshot.form.password = ""
    snapshot.form.verify = ""
    noteReadied(
      await act(
        action,
        OnboardingWords.creating,
        input: secret
      )
    )
    secret.resetBytes(in: 0..<secret.count)
  }

  /// Remembers whether the helper just chosen or created was readied for its first login, from the command's reply.
  ///
  /// - Parameter outcome: The command's outcome, when it ran.
  private func noteReadied(_ outcome: Outcome?) {
    guard let outcome, outcome.ok else { return }
    snapshot.helperReadied = outcome.decode(HelperChoice.self)?.readied ?? true
  }

  /// Gives up the helper picked but never set up and goes back to the choice: the list, never the account form,
  /// even when the account was just created there.
  func chooseAnother() async {
    guard let outcome = await act(.forgetHelper, OnboardingWords.backToChoice), outcome.ok else { return }
    snapshot.choices.creating = false
  }
}
