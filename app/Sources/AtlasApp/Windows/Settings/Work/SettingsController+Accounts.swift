import AtlasService
import AtlasUI

/// The Accounts pane's work: adding an app the person chooses, taking an account off the list once they confirm, and
/// the note read once. Each change goes through the viewer service, then the helper's status every window shares is
/// read again, so the list shows what Atlas now holds; a failure says why under the list.
extension SettingsController {
  /// Lets the person choose an app, then adds it to the accounts.
  func addApp() async {
    guard !session.accountsWorking, let app = await dependencies.chooseApp() else { return }
    await changeAccounts {
      try await self.dependencies.links.addAccount(app.bundleID, app.name)
    }
  }

  /// Takes the account with `id` off the list; the person confirmed. One no longer listed is left alone.
  ///
  /// - Parameter id: The account, as the service names it.
  func removeAccount(_ id: String) async {
    guard
      let account = dependencies.links.status().status.overview.accounts?
        .first(where: { $0.id == id })
    else { return }
    await changeAccounts {
      try await self.dependencies.links.removeAccount(account)
    }
  }

  /// The person read the note that Atlas added an account: it stays read for that account, across launches.
  func dismissAccountNote() {
    guard let note = dependencies.links.status().status.overview.accountNote else { return }
    accountNoteRead = note.added
    dependencies.preferences.saveAccountNoteRead(note.added)
  }

  /// Runs one change to the list at a time, saying why one failed, then reads the status again.
  ///
  /// - Parameter change: The change.
  private func changeAccounts(_ change: @MainActor () async throws -> Void) async {
    session.accountsWorking = true
    say(nil, in: .accounts)
    do {
      try await change()
    } catch {
      say(SettingsWords.accountsFailed(error.localizedDescription), in: .accounts, kind: .problem)
    }
    await dependencies.links.readStatus()
    session.accountsWorking = false
  }
}
