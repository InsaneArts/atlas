import SwiftUI

/// A scroll's mark: a pill in the mark's colour with an arrow the way the page moved, inside a white halo.
struct MarkScrollPill: View {
  /// The way the page moved.
  let arrow: ViewerScrollArrow
  /// How the action went.
  let tone: ViewerMarkTone
  /// How large it draws.
  let style: ActionMarkStyle

  /// The pill, standing along the way it scrolled.
  var body: some View {
    Capsule()
      .fill(tone.color)
      .overlay(alignment: .bottom) {
        Image(systemName: SymbolName.scrollArrow.rawValue)
          .resizable()
          .scaledToFit()
          .fontWeight(.bold)
          .foregroundStyle(Color.onMark)
          .padding(style.pill / 4)
          .frame(width: style.pill, height: style.pill)
      }
      .padding(Stroke.thick)
      .background(Capsule().fill(Color.markHalo))
      .background(
        Capsule()
          .fill(tone.glow)
          .padding(-style.glow)
      )
      .frame(width: style.pill, height: style.pill * ViewerLayout.markPillLength)
      .rotationEffect(turn)
  }

  /// How far the pill turns from pointing down.
  private var turn: Angle {
    switch arrow {
    case .down: .zero
    case .up: .degrees(180)
    case .left: .degrees(90)
    case .right: .degrees(-90)
    }
  }
}
