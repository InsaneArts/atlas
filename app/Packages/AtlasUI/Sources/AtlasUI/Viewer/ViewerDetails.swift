import SwiftUI

/// The Details pane beside the main pane, of the same card as the tab panes: a header with Check Again and Hide, then
/// the helper, whether it is ready to act, the picture and the session, each a group of rows straight on the pane.
struct ViewerDetails: View {
  /// What the pane shows.
  let state: ViewerDetailsState
  /// Reads how old the newest picture is as its own row draws, so the picture's age redraws that row alone.
  let newest: @MainActor () -> String
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  var body: some View {
    VStack(spacing: .zero) {
      PaneHeader(title: "Details", meta: "") {
        checkAgain
        Button {
          send(.toggleDetails)
        } label: {
          Image(systemName: "xmark")
        }
        .buttonStyle(.circle(.small))
        .help("Hide Details (⌥⌘I)")
        .accessibilityLabel("Hide Details")
      }
      ScrollView {
        VStack(alignment: .leading, spacing: Layout.groupGap) {
          DetailsGroup(title: "Helper") {
            SeparatedRows(items: state.helper) { DetailRow(name: $0.name, value: $0.value) }
          }
          if !state.ready.isEmpty {
            DetailsGroup(title: "Ready to act") {
              SeparatedRows(items: state.ready) { row in
                DetailRow(name: row.name, value: row.value, readiness: row.mark, note: row.note, isQuiet: row.isQuiet)
              }
            }
          }
          DetailsGroup(title: "Picture") {
            DetailRow(name: "Updates", value: state.updates)
            SeparatorLine()
            NewestPictureRow(newest: newest)
          }
          if !state.session.isEmpty {
            DetailsGroup(title: "Session") {
              SeparatedRows(items: state.session) { DetailRow(name: $0.name, value: $0.value) }
            }
          }
        }
        .padding(.vertical, Spacing.l)
        .padding(.horizontal, ViewerLayout.paneTextInset)
      }
      .scrollIndicators(.automatic)
    }
    .frame(maxHeight: .infinity, alignment: .top)
    .clipShape(RoundedRectangle(cornerRadius: Radius.large))
    .cardSurface()
    .accessibilityElement(children: .contain)
    .accessibilityLabel("Details")
  }

  /// Check Again: a round ↻, or a spinner in its place while the check runs.
  private var checkAgain: some View {
    Button {
      send(.checkAgain)
    } label: {
      if state.isChecking {
        Spinner(size: IconSize.xs)
      } else {
        Image(systemName: "arrow.clockwise")
      }
    }
    .buttonStyle(.circle(.small))
    .help("Check the helper again")
    .accessibilityLabel("Check Again")
  }
}

/// The pane's own measures.
private enum Layout {
  /// 18: between the groups, between the grid's 16 and 20, as the board's groups stand apart.
  static let groupGap: CGFloat = 18
}

#if DEBUG
  #Preview("Details") {
    if let details = PreviewStates.viewerDetails.details {
      ViewerDetails(state: details, newest: { details.newest }) { _ in }
        .frame(width: ViewerLayout.detailsWidth, height: PreviewStates.viewerSize.height)
        .sampleSurface()
    }
  }

  #Preview("Some checks not passing") {
    if let details = PreviewStates.viewerDetailsChecks.details {
      ViewerDetails(state: details, newest: { details.newest }) { _ in }
        .frame(width: ViewerLayout.detailsWidth, height: PreviewStates.viewerSize.height)
        .sampleSurface()
    }
  }
#endif
