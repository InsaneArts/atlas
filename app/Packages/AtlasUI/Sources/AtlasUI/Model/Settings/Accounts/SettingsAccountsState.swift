/// The Accounts pane: the accounts Atlas lists for the helper, a note once when Atlas added one, a line while pictures
/// of agent actions are off, and what the last change said.
public struct SettingsAccountsState: Equatable, Sendable {
  /// The accounts, in the order the service lists them; nil when Atlas can't read its list.
  public var rows: [SettingsAccountRow]?
  /// What the pane says when there are none.
  public var empty: EmptyMessageState
  /// The note Atlas shows once after it added an account, such as "Added github.com after you typed a password
  /// there."; nil for none.
  public var note: String?
  /// The account the note is about, by its row's id, whose level Change… sets; nil for none.
  public var noteAccount: String?
  /// What the pane says under the list while pictures of agent actions are off in General, such as "Activity won't
  /// keep pictures of what agents do here."; nil while they are kept.
  public var picturesOff: String?
  /// What the last change said, under the list; nil for nothing.
  public var notice: SettingsNote?
  /// Whether a change runs now, so the buttons wait.
  public var isWorking: Bool

  /// Creates the pane's state.
  ///
  /// - Parameters:
  ///   - rows: The accounts; nil when Atlas can't read its list.
  ///   - empty: What the pane says when there are none.
  ///   - note: The note Atlas shows once after it added an account.
  ///   - noteAccount: The account the note is about; none by default.
  ///   - picturesOff: What the pane says while pictures are off; none by default.
  ///   - notice: What the last change said.
  ///   - isWorking: Whether a change runs now.
  public init(
    rows: [SettingsAccountRow]?,
    empty: EmptyMessageState,
    note: String? = nil,
    noteAccount: String? = nil,
    picturesOff: String? = nil,
    notice: SettingsNote? = nil,
    isWorking: Bool = false
  ) {
    self.rows = rows
    self.empty = empty
    self.note = note
    self.noteAccount = noteAccount
    self.picturesOff = picturesOff
    self.notice = notice
    self.isWorking = isWorking
  }
}
