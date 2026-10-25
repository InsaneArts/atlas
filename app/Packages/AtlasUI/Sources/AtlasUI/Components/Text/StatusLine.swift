import SwiftUI

/// One line that says how things stand: a small mark and a sentence, in the same place on every screen. Work
/// shows a spinner; a problem is orange; done is green. A line that says work runs waits ``Motion/noteAppear``'s
/// delay before it shows, so work that ends sooner never flashes it, while a problem or a success shows at once.
/// A new sentence swaps in place of the old by opacity, the same under Reduce Motion.
struct StatusLine: View {
  /// How things stand.
  enum Kind: Equatable, Sendable {
    /// Work is running.
    case working
    /// Something needs the person.
    case problem
    /// It is done.
    case done
  }

  /// How large the sentence is set.
  enum Size: Sendable {
    /// Callout, as a screen's status line.
    case regular
    /// Caption, as small print under a group.
    case small

    /// The sentence's style.
    var textStyle: TextStyle {
      self == .regular ? .callout : .caption
    }

    /// The mark's inset from the top, so the 13-point mark centres on the first line.
    var markInset: CGFloat {
      self == .regular ? Layout.markInset : Layout.smallMarkInset
    }
  }

  /// 13: the mark's side, a step under the callout's line.
  static let markSide: CGFloat = 13

  /// How things stand.
  let kind: Kind
  /// The sentence.
  let text: String
  /// How large the sentence is set; callout by default.
  var size = Size.regular
  /// Whether the mark pops in as the line appears, such as About's green check.
  var popsMark = false
  /// The sentence's colour when specified, leaving the mark in its status colour.
  var textColor: Color?

  /// The mark, then the sentence.
  var body: some View {
    HStack(alignment: .top, spacing: Layout.gap) {
      mark
        .padding(.top, size.markInset)
      Text(text)
        .textStyle(size.textStyle)
        .foregroundStyle(textColor ?? Self.color(kind))
        .multilineTextAlignment(.leading)
        .fixedSize(horizontal: false, vertical: true)
        .swapsText(on: text)
    }
    .animation(Motion.textSwap, value: kind)
    .appearsAsNote(holds: kind == .working)
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(kind.spokenValue)
    .accessibilityValue(text)
  }

  /// The mark, popping in as the line appears where asked.
  @ViewBuilder private var mark: some View {
    if popsMark {
      OutcomeMark(kind: kind)
        .popsIn()
    } else {
      OutcomeMark(kind: kind)
    }
  }

  /// The sentence's colour: orange for a problem, green once done, secondary while work runs.
  ///
  /// - Parameter kind: How things stand.
  /// - Returns: The colour.
  nonisolated static func color(_ kind: Kind) -> Color {
    switch kind {
    case .working: .textSecondary
    case .problem: .textAttention
    case .done: .textSuccess
    }
  }
}

/// The status line's own measures for its mark and text.
private enum Layout {
  /// 7: between the mark and the sentence.
  static let gap: CGFloat = 7
  /// 1.5: the mark's inset from the top, so a 13-point mark centres on the first 16-point line.
  static let markInset: CGFloat = 1.5
  /// 0.5: the same on a 14-point caption line.
  static let smallMarkInset: CGFloat = 0.5
}

#if DEBUG
  #Preview("Kinds") {
    StatusLineSamples(.kinds)
  }

  #Preview("Interactive") {
    @Previewable @State var kind = StatusLine.Kind.working
    InteractivePreview {
      Picker(String("Kind"), selection: $kind) {
        Text(verbatim: "Working")
          .tag(StatusLine.Kind.working)
        Text(verbatim: "Problem")
          .tag(StatusLine.Kind.problem)
        Text(verbatim: "Done")
          .tag(StatusLine.Kind.done)
      }
      .pickerStyle(.segmented)
      .fixedSize()
    } content: {
      StatusLine(kind: kind, text: StatusLineSamples.sentence(kind))
    }
  }
#endif
