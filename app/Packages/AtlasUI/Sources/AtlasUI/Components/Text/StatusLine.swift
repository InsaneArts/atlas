import SwiftUI

/// One line that says how things stand (v3 D4): a small mark and a sentence, in the same place on every screen. Work
/// shows a spinner, which waits before it shows; a problem is orange; done is green. A new sentence replaces the old
/// with a blur (v3 R11); under Reduce Motion it fades.
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

  /// 13: the mark's side (v3 D4), a step under the callout's line.
  static let markSide: CGFloat = 13

  /// How things stand.
  let kind: Kind
  /// The sentence.
  let text: String
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The mark, then the sentence.
  var body: some View {
    HStack(alignment: .top, spacing: Layout.gap) {
      OutcomeMark(kind: kind)
        .padding(.top, Layout.markInset)
      Text(text)
        .textStyle(.callout)
        .foregroundStyle(Self.color(kind))
        .multilineTextAlignment(.leading)
        .fixedSize(horizontal: false, vertical: true)
        .id(text)
        .transition(Motion.transition(AnyTransition(.blurReplace(.downUp)), reduceMotion: reduceMotion))
    }
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: text)
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: kind)
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(kind.spokenValue)
    .accessibilityValue(text)
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

/// v3's status line (`.stl`).
private enum Layout {
  /// 7: between the mark and the sentence.
  static let gap: CGFloat = 7
  /// 1.5: the mark's inset from the top, so a 13-point mark centres on the first 16-point line.
  static let markInset: CGFloat = 1.5
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
