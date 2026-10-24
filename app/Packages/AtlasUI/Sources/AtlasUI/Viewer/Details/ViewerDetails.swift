import SwiftUI

/// Details, an inspector beside the stage (v3 V3): a quiet card, 280 wide, its header with Check Again, then the
/// helper, whether it is ready to act, the picture and the session, each a group of facts at 12 points, scrolling when
/// the window is short. The bar's `sidebar.right` button shows and hides it.
struct ViewerDetails: View {
  /// What the inspector shows.
  let state: ViewerDetailsState
  /// Reads how old the newest picture is as its own row draws, so the picture's age redraws that row alone.
  let newest: @MainActor () -> String
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The header and the groups on the inspector's card, scrolling when short.
  var body: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.detailsInset) {
      ViewerDetailsHeader(isChecking: state.isChecking, send: send)
      ScrollView {
        ViewerDetailsGroups(state: state, newest: newest)
      }
      .scrollIndicators(.automatic)
    }
    .padding(ViewerLayout.detailsInset)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .background {
      RoundedRectangle(cornerRadius: Radius.large)
        .filledOutline(Color.fillCard, border: Color.separator)
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Details"))
  }
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

  #Preview("Interactive") {
    @Previewable @State var isChecking = false
    if let details = PreviewStates.viewerDetailsChecks.details {
      InteractivePreview {
        Toggle(String("Checking"), isOn: $isChecking)
      } content: {
        ViewerDetails(
          state: {
            var shown = details
            shown.isChecking = isChecking
            return shown
          }(),
          newest: { details.newest }
        ) { event in
          if event == .checkAgain { isChecking.toggle() }
        }
        .frame(width: ViewerLayout.detailsWidth, height: PreviewStates.viewerSize.height)
      }
    }
  }
#endif
