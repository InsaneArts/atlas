import AtlasService
import AtlasUI
import Foundation

extension SettingsPresenter {
  /// The Accounts pane: each account Atlas lists, by name, with when it was added; the note once after Atlas added one
  /// the person typed a password in; and what the last change said.
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
          detail: SettingsWords.accountDetail(account, now: snapshot.now, calendar: snapshot.calendar)
        )
      }
    }
    let note = overview.accountNote
      .flatMap { $0.added > snapshot.accountNoteRead ? SettingsWords.accountAdded($0.name) : nil }
    return SettingsAccountsState(
      rows: rows,
      empty: SettingsWords.accountsNone,
      note: note,
      notice: notice(snapshot, in: .accounts, busy: snapshot.session.accountsWorking),
      isWorking: snapshot.session.accountsWorking
    )
  }
}
