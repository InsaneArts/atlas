#if DEBUG
  import AppKit
  import SwiftUI
  import Testing

  @testable import AtlasUI

  /// The Accounts pane in AppKit's hosting view, which only this test target imports: AtlasUI itself never does.
  @MainActor
  @Suite("AccountsPane")
  struct AccountsPaneTests {
    @Test("what a change said shows with no accounts, and when the list can't be read, as it does under a list")
    func aChangesLineShowsInEveryState() {
      let failed = SettingsNote(text: "Atlas couldn't change its accounts list. The list is full.", kind: .problem)
      for rows in [[], nil, PreviewStates.settingsAccountsPane.rows] as [[SettingsAccountRow]?] {
        var quiet = PreviewStates.settingsAccountsPane
        quiet.rows = rows
        var said = quiet
        said.notice = failed
        #expect(Self.height(said) > Self.height(quiet), "\(String(describing: rows?.count)) accounts")
      }
    }

    /// How tall the pane draws `state` at the Settings window's width.
    private static func height(_ state: SettingsAccountsState) -> CGFloat {
      let host = NSHostingView(
        rootView: AccountsPane(state: state) { _ in }
          .frame(width: SettingsLayout.formWidth)
      )
      return host.fittingSize.height
    }
  }
#endif
