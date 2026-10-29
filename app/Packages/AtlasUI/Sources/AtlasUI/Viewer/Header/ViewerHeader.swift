import SwiftUI

/// The content card's header, one 38-point line over the stage: who has the helper in semibold, such as "Claude Code ·
/// understudy", and what they do with it after them in grey, such as "has the helper"; or "You" and "have control of
/// the helper"; or the helper's state alone. The line swaps as one with no overlap as either part changes
/// (``LineSwap``), the old words fading out before the new fade in, so the part that stays never shifts as the other
/// changes width; a new colour for the same words lands in place. A long name is cut short in
/// its middle, so its project still shows, after the grey words have been cut short first; in a narrow card the grey
/// words fade out altogether, leaving the line to the name. While the sidebar is hidden its words start past the window
/// buttons and the sidebar toggle in the card's corner. VoiceOver reads it as one.
struct ViewerHeader: View {
  /// What the header says.
  let state: ViewerHeaderState
  /// The inset before its words: its own beside the sidebar, more while the sidebar is hidden, so the words clear the
  /// window buttons and the sidebar toggle; it moves with the sidebar.
  var leading = ViewerLayout.headerInset
  /// How much of its room the line holds back at its trailing end while the sidebar moves, so its words are not laid
  /// out again as the card's edge travels (``SidebarSlide``); none at rest.
  var hold: CGFloat = .zero

  /// The two parts, on one line, swapping together.
  var body: some View {
    LineSwap(state, id: \.words) { header in
      HStack(spacing: ViewerLayout.headerGap) {
        Text(header.who)
          .font(Typography.font(.body, weight: .semibold))
          .foregroundStyle(header.tone.wordColor)
          .truncationMode(.middle)
          .layoutPriority(1)
        if header.showsWhat, !header.what.isEmpty {
          Text(header.what)
            .font(Typography.font(.body))
            .foregroundStyle(Color.textSecondary)
            .truncationMode(.tail)
        }
      }
    }
    .lineLimit(1)
    .padding(.leading, leading)
    .padding(.trailing, ViewerLayout.headerInset + hold)
    .frame(maxWidth: .infinity, alignment: .leading)
    .frame(height: ViewerLayout.headerHeight)
    .accessibilityElement(children: .combine)
    .accessibilityAddTraits(.isHeader)
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: .zero) {
      ViewerHeader(state: PreviewStates.viewerWorking.header)
      ViewerHeader(state: PreviewStates.viewerControl.header)
      ViewerHeader(state: PreviewStates.viewerNeedsYou.header)
    }
    .frame(width: SampleWidth.column * 3)
    .background(Color.fillContent)
    .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      ViewerHeader(state: state.header)
        .frame(width: SampleWidth.column * 3)
        .background(Color.fillContent)
    }
  }
#endif
