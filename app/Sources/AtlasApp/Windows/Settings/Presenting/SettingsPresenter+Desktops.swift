import AtlasService
import AtlasStatus
import AtlasUI

extension SettingsPresenter {
  /// The sidebar's first row once the desktops are read and Second Login is not the one there is: Desktops with two or
  /// more, "Ready" under it unless one needs setup, which it names; the one VM by its title, saying how it stands. Nil
  /// while Second Login is the one desktop, or none was read, when the row is Helper's as before.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The row, or nil.
  static func desktopsRow(_ snapshot: SettingsSnapshot) -> SettingsSidebarRow? {
    guard let listed = snapshot.desktops?.desktops, let first = listed.first else { return nil }
    guard listed.count > 1 else {
      guard first.kind == .vm else { return nil }
      let standing = DesktopStanding.of(first)
      let word =
        switch standing {
        case .needsSetup: SettingsWords.helperNeedsSetup
        case .maintenance: Words.standing(.maintenance) ?? SettingsWords.helperReady
        case .unknown: SettingsWords.helperConnecting
        case .ready, .running, .starting, .stopping, .stopped: SettingsWords.helperReady
        }
      return SettingsSidebarRow(
        page: .helper,
        title: Words.title(of: first),
        subtitle: word,
        tone: tone(standing)
      )
    }
    let needing = listed.first { DesktopStanding.of($0).needsSetup }
    return SettingsSidebarRow(
      page: .helper,
      title: SettingsWords.desktops,
      subtitle: needing.map { SettingsWords.needsSetup(Words.title(of: $0)) } ?? SettingsWords.helperReady,
      tone: needing == nil ? .ready : .attention
    )
  }

  /// The Desktops page, once more than one desktop is set up: each with its title, the default tagged, and what it is
  /// with how it stands; nil while one is, or none was read.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The page, or nil.
  static func desktopsPage(_ snapshot: SettingsSnapshot) -> SettingsDesktopsState? {
    guard let listed = snapshot.desktops?.desktops, listed.count > 1 else { return nil }
    return SettingsDesktopsState(
      rows: listed.map { entry in
        SettingsDesktopRow(
          id: entry.id,
          title: Words.title(of: entry),
          detail: SettingsWords.detail(of: entry, standing: DesktopStanding.of(entry)),
          isDefault: entry.isDefault,
          isVM: entry.kind == .vm
        )
      },
      notice: notice(snapshot, in: .helper, busy: false)
    )
  }

  /// Whether Add a Desktop… shows at the Helper page's foot: while Second Login is the one desktop listed.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: Whether it shows.
  static func addsDesktop(_ snapshot: SettingsSnapshot) -> Bool {
    guard let listed = snapshot.desktops?.desktops else { return false }
    return listed.count == 1 && listed.first?.kind == .user
  }

  /// A desktop's dot: green while agents can use it, blue while the person has a VM open for maintenance, orange while
  /// it needs setup, grey otherwise.
  ///
  /// - Parameter standing: How it stands.
  /// - Returns: The tone.
  static func tone(_ standing: DesktopStanding) -> StatusTone {
    switch standing {
    case .ready, .running: .ready
    case .maintenance: .link
    case .needsSetup: .attention
    case .starting, .stopping, .stopped, .unknown: .quiet
    }
  }
}
