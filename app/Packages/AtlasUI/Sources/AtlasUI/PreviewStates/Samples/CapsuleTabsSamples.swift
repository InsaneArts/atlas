#if DEBUG
  import SwiftUI

  /// The capsule tabs' samples: the main window's four tabs with the first and then the last selected.
  struct CapsuleTabsSamples: View {
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
      CapsuleTab(id: Pane.screen, title: "Screen"),
      CapsuleTab(id: Pane.activity, title: "Activity"),
      CapsuleTab(id: Pane.apps, title: "Apps"),
      CapsuleTab(id: Pane.screenshots, title: "Screenshots"),
    ]

    var body: some View {
      VStack(spacing: Spacing.m) {
        CapsuleTabs(state: CapsuleTabsState(tabs: Self.tabs, selected: .screen, label: "What you see")) { _ in }
        CapsuleTabs(state: CapsuleTabsState(tabs: Self.tabs, selected: .screenshots, label: "What you see")) { _ in }
      }
      .sampleSurface()
    }
  }
#endif
