import SwiftUI

/// One selectable row among several, with a radio on the right.
struct ChoiceRow: View {
  /// The choice.
  let title: String
  /// What it means.
  let detail: String
  /// What the disc on the left shows.
  let mark: AccountMark
  /// Whether it is the one chosen.
  let selected: Bool
  /// Chooses it.
  let pick: @MainActor () -> Void

  var body: some View {
    Button(action: pick) {
      HStack(spacing: Spacing.m) {
        disc
          .accessibilityHidden(true)
        TitleDetail(title: title, detail: detail, wraps: true)
        Spacer(minLength: .zero)
        Circle()
          .strokeBorder(
            selected ? Color.accent : Color.strokeControl,
            lineWidth: selected ? Layout.selectedRing : Stroke.emphasis
          )
          .frame(width: IconSize.s, height: IconSize.s)
      }
      .padding(.horizontal, Spacing.mPlus)
      .padding(.vertical, Spacing.m)
      .cardSurface(selected ? .accent : .neutral)
      .contentShape(RoundedRectangle(cornerRadius: Radius.large))
    }
    .buttonStyle(.plain)
    .foregroundStyle(Color.textPrimary)
    .accessibilityAddTraits(selected ? [.isSelected] : [])
  }

  /// A symbol on a control's fill, or an account's letter on an avatar's.
  private var disc: Disc {
    switch mark {
    case .symbol(let symbol): Disc(content: .symbol(symbol), tone: .neutral, size: .avatar)
    case .letter(let letter): Disc(content: .letter(letter), tone: .avatar, size: .avatar)
    }
  }
}

/// The row's own measures.
private enum Layout {
  /// 5: a selected radio's ring, thick enough to leave only a dot of the ground inside.
  static let selectedRing: CGFloat = 5
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.choice)
  }

  #Preview("Interactive") {
    @Previewable @State var picked = "new"
    InteractivePreview {
      VStack(spacing: Spacing.s) {
        ChoiceRow(
          title: String("Create a new account"),
          detail: String("Recommended. A standard account named Atlas."),
          mark: .symbol("plus"),
          selected: picked == "new"
        ) { picked = "new" }
        ChoiceRow(
          title: String("Test Account"),
          detail: String("testaccount · standard account on this Mac"),
          mark: .letter("T"),
          selected: picked == "testaccount"
        ) { picked = "testaccount" }
      }
      .frame(width: SampleWidth.column)
    }
  }
#endif
