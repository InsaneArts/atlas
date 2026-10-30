import SwiftUI

/// A filled circle holding one symbol, letter or number: a step's number, an actor's initial, an account's avatar.
struct Disc: View {
  /// What the disc holds.
  enum Content: Equatable, Sendable {
    /// An SF Symbol, such as `checkmark`.
    case symbol(String)
    /// A letter, such as an account's initial.
    case letter(String)
    /// A step's number.
    case number(Int)
  }

  /// How the disc is painted: its fill and the colour of what it holds.
  enum Tone: Sendable {
    /// Blue with white: the step to do now.
    case accent
    /// A soft blue with blue: the person's own action.
    case accentSoft
    /// A control's fill with white: someone else's action, or a symbol beside an account.
    case neutral
    /// An avatar's grey with white: an account.
    case avatar
    /// Green with a dark glyph: done.
    case success
    /// Orange with a dark glyph: something failed or needs the person.
    case attention
    /// A faint fill with grey: a step not reached yet.
    case waiting
  }

  /// How large the disc is.
  enum Size: Sendable {
    /// 18: a mark beside a row's text.
    case mark
    /// 24: a step's or an activity's disc.
    case regular
    /// 34: an avatar in a choice row.
    case avatar
    /// 48: the asking account's avatar in the helper's allow pairing, beside Atlas's tile of the same size.
    case pairing
  }

  /// What the disc holds.
  let content: Content
  /// How it is painted.
  let tone: Tone
  /// How large it is.
  var size = Size.regular

  /// The disc in its tone, its symbol, letter or number in the middle.
  var body: some View {
    ZStack {
      Circle()
        .fill(fill)
      glyph
        .foregroundStyle(ink)
    }
    .frame(width: side, height: side)
  }

  /// The symbol, letter or number, in the disc's font.
  @ViewBuilder private var glyph: some View {
    switch content {
    case .symbol(let name):
      Image(systemName: name)
        .font(font)
    case .letter(let letter):
      Text(letter)
        .font(font)
    case .number(let number):
      Text(verbatim: String(number))
        .font(font)
    }
  }

  /// The glyph's font at each size: heavy in a mark, so it reads at 18 points; bold in a regular disc, with a
  /// number a step larger than a letter, as the steps have always set it; semibold in an avatar.
  private var font: Font {
    switch (size, content) {
    case (.mark, _): Typography.font(.footnote, weight: .heavy)
    case (.regular, .number): Typography.font(.callout, weight: .bold)
    case (.regular, _): Typography.font(.caption, weight: .bold)
    case (.avatar, _): Typography.font(.body, weight: .semibold)
    case (.pairing, _): Typography.font(.title3, weight: .semibold)
    }
  }

  private var side: CGFloat {
    switch size {
    case .mark: IconSize.sPlus
    case .regular: IconSize.m
    case .avatar: IconSize.xl
    case .pairing: DiscLayout.pairingSide
    }
  }

  private var fill: Color {
    switch tone {
    case .accent: .accent
    case .accentSoft:
      Color.accent
        .opacity(DiscLayout.accentSoft)
    case .neutral: .fillControl
    case .avatar: .fillAvatar
    case .success: .success
    case .attention: .attention
    case .waiting: .fillControl
    }
  }

  private var ink: Color {
    switch tone {
    case .accent: .onAccent
    case .accentSoft: .accent
    case .neutral, .avatar: .textPrimary
    case .success: .onSuccess
    case .attention: .onAttention
    case .waiting: .textSecondary
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    IndicatorSamples(.disc)
  }
#endif
