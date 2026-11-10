#if DEBUG
  import AppKit
  import SwiftUI
  import Testing

  @testable import AtlasUI

  /// The Privacy page in AppKit's hosting view, which only this test target imports: AtlasUI itself never does.
  @MainActor
  @Suite("Privacy read failure")
  struct PrivacyReadFailureTests {
    @Test("a reading that failed says why on Privacy even though neither row can show")
    func aFailedReadingShowsWithNoRows() {
      var quiet = PreviewStates.settingsFilesUnread
      quiet.status = nil
      #expect(Self.height(PreviewStates.settingsFilesUnread) > Self.height(quiet))
    }

    /// How tall the page draws `state` at a page's width.
    private static func height(_ state: SettingsFilesState) -> CGFloat {
      let host = NSHostingView(
        rootView: PrivacyPage(state: state) { _ in }
          .frame(width: SettingsLayout.pageWidth)
      )
      return host.fittingSize.height
    }
  }
#endif
