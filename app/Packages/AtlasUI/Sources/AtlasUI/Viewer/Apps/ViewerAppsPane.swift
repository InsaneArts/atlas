import SwiftUI

/// The Apps tab (v3 V9): the apps open in the helper, front to back, each a group with its real icon and its windows
/// with their sizes, with a search over their names; or why none shows. Show on Screen shows on the window row under
/// the pointer only.
struct ViewerAppsPane: View {
  /// What the pane shows.
  let state: ViewerAppsState
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// The window row under the pointer; nil for none.
  @State private var hoveredWindow: Int?

  /// The header with the search, over the apps or why there are none.
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

  /// The apps' groups, or why there are none.
  @ViewBuilder private var content: some View {
    if let empty = state.empty {
      EmptyMessage(state: empty, size: .stage, symbol: .windows)
    } else {
      ScrollView {
        LazyVStack(spacing: Spacing.sPlus) {
          ForEach(state.apps) { app in
            AppCard(app: app, hoveredWindow: hoveredWindow, hover: hover, send: send)
          }
        }
        .padding(.top, ViewerLayout.appsTop)
        .padding(.bottom, Spacing.sPlus)
        .padding(.horizontal, ViewerLayout.paneInset)
      }
      .scrollIndicators(.automatic)
    }
  }

  /// Notes the window row under the pointer: the one it enters, and none once it leaves the one it is on, whichever
  /// of the two the system reports first as it moves from row to row.
  ///
  /// - Parameters:
  ///   - window: The window whose row the pointer entered or left.
  ///   - isOver: Whether it entered.
  private func hover(_ window: Int, _ isOver: Bool) {
    if isOver {
      hoveredWindow = window
    } else if hoveredWindow == window {
      hoveredWindow = nil
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
