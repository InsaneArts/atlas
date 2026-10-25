import SwiftUI

/// An agent's original glyph on its brand card, shared by setup and Settings.
struct AgentMark: View {
  /// Which agent's mark to draw.
  let kind: AgentMarkKind

  /// The brand card, its colour layers, highlight and fine rim.
  var body: some View {
    let card = RoundedRectangle(cornerRadius: Radius.extraSmall, style: .continuous)
    ZStack {
      card
        .fill(LinearGradient(colors: kind.colors, startPoint: .top, endPoint: .bottom))
      card
        .fill(LinearGradient(colors: [.brandCardHighlight, .clear], startPoint: .top, endPoint: .center))
      ForEach(kind.paths.indices, id: \.self) { index in
        AgentGlyph(data: kind.paths[index], layers: kind.paths)
          .fill(kind.inks[index], style: FillStyle(eoFill: true))
      }
      card
        .strokeBorder(Color.brandCardRim, lineWidth: Stroke.hairline)
        .padding(Stroke.hairline)
      card
        .strokeBorder(Color.brandCardEdge, lineWidth: Stroke.hairline)
    }
    .frame(width: IconSize.mPlus, height: IconSize.mPlus)
    .shadow(color: Color.brandCardShadow, radius: BrandMark.shadowRadius, y: Stroke.hairline)
    .accessibilityHidden(true)
  }
}
