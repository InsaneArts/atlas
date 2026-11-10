import AtlasService
import AtlasUI
import Foundation

extension SettingsPresenter {
  /// The Accounts page: each account Atlas lists, by name, with its level and when it was added; the note once after
  /// Atlas added one the person typed a password in, with the account it is about; a line while Activity keeps no
  /// pictures of agent actions; and what the last change asked from the page said.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The page.
  static func accounts(_ snapshot: SettingsSnapshot) -> SettingsAccountsState {
    let overview = snapshot.facts.status.overview
    let rows = overview.accounts.map { accounts in
      accounts.map { account in
        SettingsAccountRow(
          id: account.id,
          letter: letter(account),
          title: account.name,
          detail: SettingsWords.accountRowDetail(account, now: snapshot.now, calendar: snapshot.calendar),
          level: AccountLevelChoice(account.level),
          isSite: account.kind == .site,
          icon: snapshot.accountIcons[account.id]
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
      picturesOff: snapshot.pictures.shown.isOn ? nil : SettingsWords.accountsPicturesOff,
      notice: notice(snapshot, in: .accounts, busy: snapshot.session.accountsWorking),
      isWorking: snapshot.session.accountsWorking
    )
  }

  /// The page of the account shown, if one shows: its letter, name and what it is, its level with what that lets
  /// agents do, and what the last change on it said. While Atlas can't read its list the page keeps the account as it
  /// was last listed, its level unknown and its choices waiting; an account no longer listed has no page.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The page, or nil.
  static func accountPage(_ snapshot: SettingsSnapshot) -> SettingsAccountPageState? {
    guard let id = route(snapshot).account else { return nil }
    let listed = snapshot.facts.status.overview.accounts
    let account = listed.map { $0.first { $0.id == id } } ?? snapshot.session.knownAccounts[id]
    guard let account else { return nil }
    // A level being set shows at once; the reading after its change says what Atlas holds.
    let changing = snapshot.session.changingLevel[id]
    let held = changing ?? account.level
    let level = listed == nil ? nil : AccountLevelChoice(held)
    return SettingsAccountPageState(
      id: id,
      letter: letter(account),
      title: account.name,
      kindLine: SettingsWords.accountKindLine(account, now: snapshot.now, calendar: snapshot.calendar),
      isSite: account.kind == .site,
      level: level,
      levelLine: level == nil ? "" : SettingsWords.levelLine(held),
      isEnabled: level != nil && !snapshot.session.accountsWorking,
      notice: notice(snapshot, on: .account(id), busy: snapshot.session.accountsWorking),
      icon: snapshot.accountIcons[id],
      isChanging: changing != nil
    )
  }

  /// What the last change asked from an account's page said, whichever route shows now, so VoiceOver hears it as it
  /// lands even after the person left the page; a waiting note only while the change runs.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The note, or nil.
  static func accountNotice(_ snapshot: SettingsSnapshot) -> SettingsNote? {
    guard let route = snapshot.session.notice?.route, route.account != nil else { return nil }
    return notice(snapshot, on: route, busy: snapshot.session.accountsWorking)
  }

  /// Where the window stands, an account's page Atlas no longer lists gone back to Accounts. While Atlas can't read its
  /// list, the route stays as it is.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The route.
  static func route(_ snapshot: SettingsSnapshot) -> SettingsRoute {
    guard let listed = snapshot.facts.status.overview.accounts else { return snapshot.session.route }
    return snapshot.session.route.kept(accounts: Set(listed.map(\.id)))
  }

  /// An account's first letter, as its disc shows it.
  ///
  /// - Parameter account: The account.
  /// - Returns: The letter, upper case.
  static func letter(_ account: ViewerStatus.Account) -> String {
    String(account.name.prefix(1))
      .uppercased()
  }
}
