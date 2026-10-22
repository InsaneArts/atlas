#if DEBUG
  import SwiftUI

  /// The icon tabs' samples: a settings window's four tabs with the first and then the last selected.
  struct IconTabsSamples: View {
    /// A settings window's panes, standing in for its own.
    enum Pane: Hashable, Sendable {
      /// The app's own choices.
      case general
      /// The helper.
      case helper
      /// The agents.
      case agents
      /// The app.
      case about
    }

    /// The four tabs, in drawing order.
    static let tabs = [
      IconTab(id: Pane.general, title: "General", icon: .symbol("gearshape")),
      IconTab(id: Pane.helper, title: "Helper", icon: .symbol("display")),
      IconTab(id: Pane.agents, title: "Agents", icon: .symbol("person.2")),
      IconTab(id: Pane.about, title: "About", icon: .orbit),
    ]

    var body: some View {
      VStack(spacing: Spacing.l) {
        IconTabs(state: IconTabsState(tabs: Self.tabs, selected: .general, label: "Settings")) { _ in }
        IconTabs(state: IconTabsState(tabs: Self.tabs, selected: .about, label: "Settings")) { _ in }
      }
      .sampleSurface()
    }
  }
#endif
