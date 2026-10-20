import SwiftUI

/// The helper, what happened, and whether all is ready, as cards of rows.
struct ViewerSidebar: View {
  /// What the sidebar shows.
  let state: ViewerSidebarState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The helper's name and reason, then the Recent, Ready, Session and Picture cards.
  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: Spacing.section) {
        SidebarAccount(account: state.account, reason: state.reason)
        SidebarSection(title: "Recent") {
          recent
        }
        SidebarSection(title: "Ready") {
          CardGroup(state.ready) { item in
            ReadinessRow(name: item.name, text: item.text, state: item.state)
          }
        }
        SidebarSection(title: "Session") {
          CardGroup(state.facts) { fact in
            FactRow(name: fact.name, value: fact.value, size: .regular)
          }
        }
        SidebarSection(title: "Picture") {
          PictureRateCard(rate: state.rate, note: state.pictureNote) { send(.setRate($0)) }
        }
      }
      .padding(Spacing.group)
    }
    .scrollIndicators(.automatic)
    .frame(width: ViewerLayout.sidebarWidth)
    .frame(maxHeight: .infinity, alignment: .top)
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Sidebar")
  }

  /// The latest actions, or a line saying none have happened yet.
  @ViewBuilder private var recent: some View {
    if state.recent.isEmpty {
      CardGroup {
        Text("Nothing yet. Actions appear here as they happen.")
          .font(Typography.font(.label))
          .foregroundStyle(Color.textTertiary)
          .padding(Spacing.xl)
          .frame(maxWidth: .infinity, alignment: .leading)
      }
    } else {
      // An entry's id is its place in the service's list of recent actions, the one identity the service gives.
      CardGroup(state.recent) { entry in
        ActivityRow(initial: entry.initial, line: entry.line, when: entry.when, kind: entry.kind)
      }
    }
  }
}

#if DEBUG
  #Preview("Watching") {
    ViewerSidebar(state: PreviewStates.viewerSidebar.sidebar) { _ in }
      .frame(height: PreviewStates.viewerSize.height)
      .sampleSurface()
  }
#endif
