#if DEBUG
  import SwiftUI

  /// A Settings page alone, as it stands on the content card: in its scrolling column, the card's width and fill.
  struct SettingsPageSample<Page: View>: View {
    /// The page.
    private let page: Page

    /// Creates the sample.
    ///
    /// - Parameter page: The page.
    init(@ViewBuilder page: () -> Page) {
      self.page = page()
    }

    /// The page on the card.
    var body: some View {
      SettingsPageScroll {
        page
      }
      .frame(width: SettingsLayout.cardWidth, height: SettingsLayout.height)
      .background(Color.fillContent)
    }
  }
#endif
