import SwiftUI

/// A footer capsule's label: its symbol, such as Take Control's hand or Stop's square, and its title, swapping together
/// with no overlap (``LineSwap``), so a symbol fades out with the words it led and in with the words it leads, never
/// over the last title's words. Its room is the widest of every label the capsule takes in turn, so Take Control,
/// Connecting…, Give Back and Recover…, or Stop and Resume, all stand at one width, the label centred in it: pressing
/// the capsule changes its words, never its size or its place. In a narrow card a label with a symbol stands as the
/// symbol alone, its title faded and given no room, while one with none keeps its words.
struct FooterLabel: View {
  /// The label shown; nil draws none.
  let label: ViewerControlLabel?
  /// Every label the capsule takes in turn, whose widest is the room.
  let room: [ViewerControlLabel]
  /// The symbol's font.
  let symbolFont: Font
  /// Whether a label with a symbol stands as the symbol alone.
  let collapsesTitles: Bool
  /// Whether the title of a label with a symbol shows; one with none always shows its words.
  let showsTitles: Bool

  /// The label's room, and the label swapping in it.
  var body: some View {
    ZStack {
      ForEach(Array(room.enumerated()), id: \.offset) { _, label in
        drawn(label)
          .hidden()
      }
      LineSwap(label, id: \.?.title) { label in
        drawn(label)
      }
    }
  }

  /// One label as drawn: its symbol, then its title.
  ///
  /// - Parameter label: The label.
  /// - Returns: The label.
  private func drawn(_ label: ViewerControlLabel?) -> some View {
    let collapses = collapsesTitles && label?.symbol != nil
    return HStack(spacing: collapses ? .zero : ViewerLayout.footerSymbolGap) {
      if let symbol = label?.symbol {
        Image(systemName: symbol.rawValue)
          .font(symbolFont)
      }
      // One letter spacing for every look, so the accent's Give Back lays out as the others do.
      Text(label?.title ?? "")
        .tracking(LetterSpacing.control)
        .fixedSize()
        .opacity(label?.symbol != nil && !showsTitles ? 0 : 1)
        .frame(width: collapses ? .zero : nil)
    }
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var isInControl = false
    InteractivePreview {
      Toggle(String("In control"), isOn: $isInControl)
    } content: {
      let room = PreviewStates.viewerMainLabels
      Button {
        isInControl.toggle()
      } label: {
        FooterLabel(
          label: isInControl ? room[2] : room[0],
          room: room,
          symbolFont: Typography.symbol(.glyph),
          collapsesTitles: false,
          showsTitles: true
        )
      }
      .buttonStyle(.capsule(isInControl ? .accent : .primary, size: .small, outline: .symbolPill))
      .animation(Motion.crossfade, value: isInControl)
    }
  }
#endif
