import SwiftUI

/// The content card: one solid card in the card's own colour, its corners nested in the window's, with a hairline edge
/// and nothing else round it; the header at its top, the stage filling the middle, and the footer at its foot. The
/// stage is the caller's: the Screen or a list from the Library, with the small screen in its corner.
struct ViewerCard<Stage: View>: View {
  /// The header's state.
  let header: ViewerHeaderState
  /// The inset before the header's words.
  let headerLeading: CGFloat
  /// How much of its room the header's line holds back while the sidebar moves (``SidebarSlide``).
  let headerHold: CGFloat
  /// The footer's state.
  let footer: ViewerFooterState
  /// How much of its room the footer's line holds back while the sidebar moves.
  let footerHold: CGFloat
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// What the card shows between its header and footer.
  let stage: Stage

  /// Creates the card.
  ///
  /// - Parameters:
  ///   - header: The header's state.
  ///   - headerLeading: The inset before the header's words; the header's own by default.
  ///   - headerHold: How much of its room the header's line holds back while the sidebar moves; none by default.
  ///   - footer: The footer's state.
  ///   - footerHold: How much of its room the footer's line holds back while the sidebar moves; none by default.
  ///   - send: Receives what the person does.
  ///   - stage: What the card shows between its header and footer.
  init(
    header: ViewerHeaderState,
    headerLeading: CGFloat = ViewerLayout.headerInset,
    headerHold: CGFloat = .zero,
    footer: ViewerFooterState,
    footerHold: CGFloat = .zero,
    send: @escaping @MainActor (ViewerEvent) -> Void,
    @ViewBuilder stage: () -> Stage
  ) {
    self.header = header
    self.headerLeading = headerLeading
    self.headerHold = headerHold
    self.footer = footer
    self.footerHold = footerHold
    self.send = send
    self.stage = stage()
  }

  /// The header, the stage and the footer, on the card.
  var body: some View {
    VStack(spacing: .zero) {
      ViewerHeader(state: header, leading: headerLeading, hold: headerHold)
      stage
        .frame(maxWidth: .infinity, maxHeight: .infinity)
      ViewerFooter(state: footer, hold: footerHold, send: send)
    }
    .background {
      shape
        .fill(Color.fillContent)
    }
    .overlay {
      shape
        .strokeBorder(Color.strokeContent, lineWidth: Stroke.hairline)
        .allowsHitTesting(false)
    }
  }

  /// The card's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: ViewerLayout.cardRadius, style: .continuous)
  }
}

#if DEBUG
  #Preview("Working") {
    ViewerCard(header: PreviewStates.viewerWorking.header, footer: PreviewStates.viewerWorking.footer) { _ in
    } stage: {
      Color.clear
    }
    .frame(width: SampleWidth.column * 3, height: SampleWidth.column * 2)
    .padding(ViewerLayout.cardInset)
    .background(Color.groundRecessed)
  }
#endif
