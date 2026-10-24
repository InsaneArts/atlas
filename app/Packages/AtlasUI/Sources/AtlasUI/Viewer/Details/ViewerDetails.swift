import SwiftUI

/// Details, an inspector beside the stage: a quiet card, 280 wide, its header with Check Again, then the helper,
/// whether it is ready to act, the picture and the session, each a group of facts at 12 points, scrolling when the
/// window is short; floating over the stage in a narrow window, the card is thick glass with a shadow. The bar's
/// `sidebar.right` button shows and hides it.
struct ViewerDetails: View {
  /// What the inspector shows.
  let state: ViewerDetailsState
  /// Whether it floats over the stage rather than standing beside it.
  var floats = false
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
      if floats {
        Color.clear
          .glass(.thick, in: shape)
          .elevation(.floating)
      } else {
        shape
          .filledOutline(Color.fillCard, border: Color.separator)
      }
    }
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: "Details"))
  }

  /// The card's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: Radius.large)
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
