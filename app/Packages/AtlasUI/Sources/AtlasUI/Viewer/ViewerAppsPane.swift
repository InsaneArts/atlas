import SwiftUI

/// The Apps tab: the apps open in the helper, front to back, each with its windows and Show on screen, with a search
/// over their names; or why none shows.
struct ViewerAppsPane: View {
  /// What the pane shows.
  let state: ViewerAppsState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  var body: some View {
    VStack(spacing: .zero) {
      PaneHeader(title: "Apps in the helper", meta: state.meta) {
        SearchField(text: state.query, prompt: "Search apps") { send(.searchApps($0)) }
      }
      content
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large))
    .cardSurface()
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Apps in the helper")
  }

  /// The apps' cards, or why there are none.
  @ViewBuilder private var content: some View {
    if let empty = state.empty {
      EmptyMessage(state: empty, ground: .pane)
        .padding(Spacing.xxl)
    } else {
      ScrollView {
        LazyVStack(spacing: Spacing.ml) {
          ForEach(state.apps) { app in
            AppCard(app: app, send: send)
          }
        }
        .padding(.vertical, Spacing.xl)
        .padding(.horizontal, ViewerLayout.paneTextInset)
      }
      .scrollIndicators(.automatic)
    }
  }
}

#if DEBUG
  #Preview("Apps") {
    if case .apps(let apps) = PreviewStates.viewerApps.pane {
      ViewerAppsPane(state: apps) { _ in }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
        .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerApps
    InteractivePreview {
      if case .apps(let apps) = state.pane {
        ViewerAppsPane(state: apps) { event in
          state = PreviewStates.viewer(state, applying: event)
        }
        .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
      }
    }
  }
#endif
