#if DEBUG
  import SwiftUI

  /// The tabs' samples: the main window's four tabs in a track by their words, then by their symbols with the last one
  /// selected, then as symbols over words, then as a segment.
  struct TabsSamples: View {
    /// The main window's tabs, standing in for its own.
    enum Pane: Hashable, Sendable {
      /// The helper's screen.
      case screen
      /// What was done.
      case activity
      /// The apps in the helper.
      case apps
      /// The agent's screenshots.
      case screenshots
    }

    /// The four tabs, in drawing order.
    static let tabs = [
      CapsuleTab(id: Pane.screen, title: "Screen", symbol: .display),
      CapsuleTab(id: Pane.activity, title: "Activity", symbol: .list),
      CapsuleTab(id: Pane.apps, title: "Apps", symbol: .windows),
      CapsuleTab(id: Pane.screenshots, title: "Screenshots", symbol: .photos),
    ]

    /// The four rows of tabs.
    var body: some View {
      VStack(spacing: Spacing.m) {
        Tabs(state: CapsuleTabsState(tabs: Self.tabs, selected: .screen, label: "What you see")) { _ in }
        Tabs(
          state: CapsuleTabsState(tabs: Self.tabs, selected: .screenshots, label: "What you see"),
          face: .symbol
        ) { _ in }
        Tabs(
          state: CapsuleTabsState(tabs: Self.tabs, selected: .activity, label: "What you see"),
          style: .symbolOverWord
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
