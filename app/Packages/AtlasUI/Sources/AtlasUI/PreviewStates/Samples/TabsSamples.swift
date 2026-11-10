#if DEBUG
  import SwiftUI

  /// The tabs' samples: three tabs in a track by their words, then by their symbols with the
  /// last one selected, then as symbols over words, then as a segment.
  struct TabsSamples: View {
    /// Three panes.
    enum Pane: Hashable, Sendable {
      /// The helper's screen.
      case screen
      /// What was done.
      case activity
      /// The agent's screenshots.
      case screenshots
    }

    /// The three tabs, in drawing order.
    static let tabs = [
      CapsuleTab(id: Pane.screen, title: "Screen", symbol: .display),
      CapsuleTab(id: Pane.activity, title: "Activity", symbol: .list),
      CapsuleTab(id: Pane.screenshots, title: "Screenshots", symbol: .photos),
    ]

    /// The rows of tabs.
    var body: some View {
      VStack(spacing: Spacing.m) {
        Tabs(state: CapsuleTabsState(tabs: Self.tabs, selected: .screen, label: "What you see")) { _ in }
        Tabs(
          state: CapsuleTabsState(tabs: Self.tabs, selected: .screenshots, label: "What you see"),
          face: .symbol
        ) { _ in }
        Tabs(
          state: CapsuleTabsState(tabs: Self.tabs, selected: .screen, label: "What you see"),
          style: .segment
        ) { _ in }
      }
      .sampleSurface()
    }
  }
#endif
