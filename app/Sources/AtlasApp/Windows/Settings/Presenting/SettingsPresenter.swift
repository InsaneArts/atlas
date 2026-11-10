import AtlasUI

/// Decides what the Settings window shows from what its controller holds and reads: a pure function of the snapshot,
/// in `SettingsPresenter+Sidebar`, `+General`, `+Helper`, `+Accounts`, `+Files`, `+Agents`, `+About` and `+Alert`.
enum SettingsPresenter {
  /// The window as it should look.
  ///
  /// - Parameter snapshot: What the controller holds and reads.
  /// - Returns: The window's state.
  static func state(_ snapshot: SettingsSnapshot) -> SettingsState {
    SettingsState(
      navigation: navigation(snapshot),
      sidebar: sidebar(snapshot),
      general: general(snapshot),
      helper: helper(snapshot),
      accounts: accounts(snapshot),
      account: accountPage(snapshot),
      accountNotice: accountNotice(snapshot),
      privacy: files(snapshot.files),
      agents: agents(snapshot),
      about: about(snapshot),
      alert: alert(snapshot),
      signIn: snapshot.signIn
    )
  }
}
