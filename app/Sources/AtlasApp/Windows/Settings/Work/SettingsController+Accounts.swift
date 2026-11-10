import AtlasService
import AtlasUI

/// The Accounts pages' work: adding an app the person chooses, setting an account's level, taking an account off the
/// list once they confirm, and the note read once. Each change goes through the viewer service, then the helper's status every window shares is
/// read again, so the list shows what Atlas now holds; a failure says why under the list.
extension SettingsController {
  /// Lets the person choose an app, then adds it to the accounts.
  func addApp() async {
    guard !session.accountsWorking, let app = await dependencies.chooseApp() else { return }
    await changeAccounts(on: .page(.accounts)) {
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
    await changeAccounts(on: .account(id)) {
      try await self.dependencies.links.removeAccount(account)
    }
  }

  /// Sets how freely agents act in the account with `id`: its page shows the level at once and the rest of it stays as
  /// it is, only the picker waiting while the change runs; one that fails goes back to the level Atlas holds and says
  /// why. One no longer listed, or whose level is changing already, is left alone.
  ///
  /// - Parameters:
  ///   - level: The level.
  ///   - id: The account, as the service names it.
  func setLevel(_ level: AccountLevel, of id: String) async {
    guard
      session.changingLevel[id] == nil,
      let account = dependencies.links.status().status.overview.accounts?
        .first(where: { $0.id == id }),
      account.level != level
    else { return }
    session.changingLevel[id] = level
    say(nil, on: .account(id))
    do {
      try await dependencies.links.setLevel(level, account)
    } catch {
      say(SettingsWords.accountsFailed(error.localizedDescription), on: .account(id), kind: .problem)
    }
    await dependencies.links.readStatus()
    keepListedAccounts()
    session.changingLevel[id] = nil
  }

  /// The person read the note that Atlas added an account: it stays read for that account, across launches.
  func dismissAccountNote() {
    guard let note = dependencies.links.status().status.overview.accountNote else { return }
    accountNoteRead = note.added
    dependencies.preferences.saveAccountNoteRead(note.added)
  }

  /// Runs one change to the list at a time, saying why one failed on the route that asked for it, then reads the
  /// status again; a route to an account the reading no longer lists leaves the window's history.
  ///
  /// - Parameters:
  ///   - route: Where the change was asked for, which says why it failed.
  ///   - change: The change.
  private func changeAccounts(on route: SettingsRoute, _ change: @MainActor () async throws -> Void) async {
    session.accountsWorking = true
    say(nil, on: route)
    do {
      try await change()
    } catch {
      say(SettingsWords.accountsFailed(error.localizedDescription), on: route, kind: .problem)
    }
    await dependencies.links.readStatus()
    keepListedAccounts()
    session.accountsWorking = false
  }
}
