import AtlasService
import AtlasUI
import Foundation

extension SettingsPresenter {
  /// The Accounts pane: each account Atlas lists, by name, with when it was added and its level; the note once after
  /// Atlas added one the person typed a password in, with the account it is about; and what the last change said.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The pane.
  static func accounts(_ snapshot: SettingsSnapshot) -> SettingsAccountsState {
    let overview = snapshot.facts.status.overview
    let rows = overview.accounts.map { accounts in
      accounts.map { account in
        SettingsAccountRow(
          id: account.id,
          letter: String(account.name.prefix(1)).uppercased(),
          title: account.name,
          detail: SettingsWords.accountDetail(account, now: snapshot.now, calendar: snapshot.calendar),
          level: AccountLevelChoice(account.level),
          isSite: account.kind == .site
        )
      }
    }
    let noted = overview.accountNote.flatMap { $0.added > snapshot.accountNoteRead ? $0 : nil }
    // The account the note is about: the newest one Atlas added after a password, as the note names it.
    let notedAccount = noted.flatMap { note in
      overview.accounts?
        .filter { $0.addedAfterPassword && $0.name == note.name }
        .max { $0.added < $1.added }
    }
    let note = noted.map { SettingsWords.accountAdded($0.name, level: notedAccount?.level ?? .ask) }
    return SettingsAccountsState(
      rows: rows,
      empty: SettingsWords.accountsNone,
      note: note,
      noteAccount: notedAccount?.id,
      open: snapshot.openAccount,
      notice: notice(snapshot, in: .accounts, busy: snapshot.session.accountsWorking),
      isWorking: snapshot.session.accountsWorking
    )
  }
}
