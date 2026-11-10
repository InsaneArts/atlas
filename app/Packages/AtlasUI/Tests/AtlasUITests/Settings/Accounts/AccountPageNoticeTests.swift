#if DEBUG
  import AppKit
  import SwiftUI
  import Testing

  @testable import AtlasUI

  /// An account's page in AppKit's hosting view, which only this test target imports: AtlasUI itself never does.
  @MainActor
  @Suite("AccountPage notice")
  struct AccountPageNoticeTests {
    @Test("a change that failed keeps saying why on the page while Atlas can't read its list")
    func aFailureShowsWhileTheListIsUnreadable() {
      #expect(
        Self.height(PreviewStates.settingsAccountFailedUnreadablePage)
          > Self.height(PreviewStates.settingsAccountUnreadablePage)
      )
    }

    /// How tall the page draws `state` at a page's width.
    private static func height(_ state: SettingsAccountPageState) -> CGFloat {
      let host = NSHostingView(
        rootView: AccountPage(state: state) { _ in }
          .frame(width: SettingsLayout.pageWidth)
      )
      return host.fittingSize.height
    }
  }
#endif
