import SwiftUI

/// The footer's line at its leading edge: the latest action led by its session's icon, with its age in a fainter grey;
/// why Take Control was refused or what went wrong, led by an orange mark; control, led by a blue dot; or the text being
/// typed, with a caret. One line in 12-point grey, cut short at its end. A new line replaces the last with no overlap
/// (``LineSwap``): the old one fades out, then the new one fades in, and a burst of lines reads as a few calm ones. The
/// same line with new words or a new age updates in place. In a very narrow card a line with an age keeps only its lead
/// and its age, its words fading out. VoiceOver reads it as one, words and all, as it updates.
struct FooterLatest: View {
  /// The newest line.
  let latest: ViewerLatestLine
  /// Whether the line keeps its words in the room it has.
  var showsWords = true
  /// How much of its room the line holds back at its trailing end while the sidebar moves, so its words are not laid
  /// out again as the card's edge travels (``SidebarSlide``); none at rest.
  var hold: CGFloat = .zero

  /// The line, swapping as it changes.
  var body: some View {
    LineSwap(latest, id: \.id) { line in
      Self.drawn(line, showsWords: showsWords)
    }
    .padding(.trailing, hold)
    .frame(maxWidth: .infinity, alignment: .leading)
    .accessibilityElement(children: .combine)
    .accessibilityLabel(Text(latest.age.isEmpty ? latest.words : "\(latest.words), \(latest.age)"))
    .accessibilityAddTraits(.updatesFrequently)
  }

  /// One line as drawn: its lead, its words with their age, and its caret.
  ///
  /// - Parameters:
  ///   - line: The line.
  ///   - showsWords: Whether the line keeps its words in the room it has.
  /// - Returns: The line.
  @ViewBuilder private static func drawn(_ line: ViewerLatestLine, showsWords: Bool) -> some View {
    HStack(spacing: ViewerLayout.footerLineGap) {
      lead(line)
      words(line, showsWords: showsWords || line.age.isEmpty)
        .font(Typography.font(.callout))
        .lineLimit(1)
      if line.caret {
        TypingCaret()
      }
    }
  }

  /// What leads the line: its session's icon, else a blue dot in control or an orange mark when something went wrong,
  /// else nothing.
  ///
  /// - Parameter line: The line.
  /// - Returns: The lead.
  @ViewBuilder private static func lead(_ line: ViewerLatestLine) -> some View {
    if let icon = line.icon {
      SessionIcon(icon: icon, side: ViewerLayout.footerIcon)
    } else {
      switch line.tone {
      case .active, .link:
        StatusDot(mark: .dot, tone: line.tone)
      case .attention:
        Image(systemName: SymbolName.attention.rawValue)
          .font(Typography.symbol(.row, weight: .medium))
          .foregroundStyle(Color.attention)
          .accessibilityHidden(true)
      case .ready, .quiet:
        EmptyView()
      }
    }
  }

  /// The words, cut short at their end, and after them their age in the fainter grey, which always shows; with no
  /// room for the words, the age alone.
  ///
  /// - Parameters:
  ///   - line: The line.
  ///   - showsWords: Whether the words show.
  /// - Returns: The words.
  private static func words(_ line: ViewerLatestLine, showsWords: Bool) -> some View {
    HStack(spacing: .zero) {
      if showsWords {
        Text(line.words)
          .foregroundStyle(line.tone == .attention ? Color.textAttention : Color.textSecondary)
          .truncationMode(.tail)
          .transition(.opacity)
      }
      if !line.age.isEmpty {
        Text(verbatim: showsWords ? " · \(line.age)" : line.age)
          .foregroundStyle(Color.textTertiary)
          .fixedSize()
      }
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .leading, spacing: Spacing.m) {
      FooterLatest(latest: PreviewStates.viewerWorking.footer.latest)
      FooterLatest(latest: PreviewStates.viewerControl.footer.latest)
      FooterLatest(latest: PreviewStates.viewerTyping.footer.latest)
      FooterLatest(latest: PreviewStates.viewerMidAction.footer.latest)
    }
    .frame(width: SampleWidth.column * 2)
    .padding(Spacing.l)
    .sampleSurface()
  }
#endif
