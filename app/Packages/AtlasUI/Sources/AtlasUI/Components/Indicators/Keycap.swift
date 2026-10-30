import SwiftUI

/// A key on the keyboard, drawn as a small cap holding its symbol or letter, so a shortcut reads as keys: ⌃ ⇧
/// ⎋ gives control back. Edged over a picture, where a cap needs its rim to stand out; soft in a list, a raised fill
/// with a solid lip under it and no edge.
struct Keycap: View {
  /// What the cap shows.
  enum Face: Equatable, Sendable {
    /// An SF Symbol.
    case symbol(SymbolName)
    /// A letter or a key's sign, such as ⌃ or ⎋.
    case text(String)
  }

  /// What the cap shows.
  let face: Face
  /// How the cap is drawn; edged by default.
  var style = Style.edged

  /// The face on its cap.
  var body: some View {
    switch style {
    case .edged: edged
    case .soft: soft
    }
  }

  /// The cap with its rim and its shaded lower edge, over a picture.
  private var edged: some View {
    label
      .font(Typography.font(.caption, weight: .medium))
      .foregroundStyle(Color.textPrimary)
      .padding(.horizontal, Spacing.xs)
      .frame(minWidth: KeycapLayout.side, minHeight: KeycapLayout.side)
      .background(
        RoundedRectangle(cornerRadius: KeycapLayout.radius, style: .continuous)
          .filledOutline(Color.fillControlRaised, border: Color.strokeStrong)
      )
      .overlay(alignment: .bottom) {
        RoundedRectangle(cornerRadius: KeycapLayout.radius, style: .continuous)
          .strokeBorder(Color.strokeShade, lineWidth: Stroke.regular)
          .mask(alignment: .bottom) {
            Rectangle()
              .frame(height: Stroke.regular)
          }
      }
  }

  /// The cap as a list draws it: a raised fill, a solid lip one point under it in place of an edge.
  private var soft: some View {
    label
      .font(Typography.font(.caption, weight: .semibold))
      .foregroundStyle(Color.textPrimary)
      .padding(.horizontal, Spacing.xs)
      .frame(minWidth: KeycapLayout.softSide, minHeight: KeycapLayout.softSide)
      .background(
        RoundedRectangle(cornerRadius: KeycapLayout.radius, style: .continuous)
          .fill(Color.fillControlRaised)
          .background(
            RoundedRectangle(cornerRadius: KeycapLayout.radius, style: .continuous)
              .fill(Color.fillControlOn)
              .offset(y: Stroke.regular)
          )
      )
  }

  /// The symbol or the text.
  @ViewBuilder private var label: some View {
    switch face {
    case .symbol(let name):
      Image(systemName: name.rawValue)
    case .text(let text):
      Text(text)
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    PictureChipSamples(.keycaps)
  }

  #Preview("Soft") {
    KeycapSamples()
  }
#endif
