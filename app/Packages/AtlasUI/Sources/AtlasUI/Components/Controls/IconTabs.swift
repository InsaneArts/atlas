import SwiftUI

/// Icon tabs side by side, as a settings window's top row sets them: each an icon over its word, the selected one on
/// a quiet fill with its icon in the link colour. Each tab is a button that VoiceOver reads as selected or not.
struct IconTabs<Tab: Hashable & Sendable>: View {
  /// The tabs and which one is selected.
  let state: IconTabsState<Tab>
  /// Receives the tab the person picks.
  let send: @MainActor (Tab) -> Void

  var body: some View {
    HStack(spacing: Spacing.xs) {
      ForEach(state.tabs) { tab in
        Button {
          send(tab.id)
        } label: {
          IconTabLabel(title: tab.title, icon: tab.icon)
        }
        .buttonStyle(IconTabStyle(isSelected: tab.id == state.selected))
        .accessibilityLabel(tab.title)
        .accessibilityAddTraits(tab.id == state.selected ? .isSelected : [])
      }
    }
    .fixedSize()
    .accessibilityElement(children: .contain)
    .accessibilityLabel(state.label)
  }
}

#if DEBUG
  #Preview("Samples") {
    IconTabsSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var selected = IconTabsSamples.Pane.general
    InteractivePreview {
      IconTabs(state: IconTabsState(tabs: IconTabsSamples.tabs, selected: selected, label: "Settings")) { tab in
        selected = tab
      }
    }
  }
#endif
