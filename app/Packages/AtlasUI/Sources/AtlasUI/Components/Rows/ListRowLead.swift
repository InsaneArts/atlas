import SwiftUI

/// What leads a ``ListRow``: a tile, an app's icon, the account picture, a letter disc, a check's mark or the
/// empty ring of a check not run yet, each in the width ``ListRow/leadWidth(_:)`` gives it. A check's mark is
/// replaced as its state changes, popping in where its owner animates the change.
struct ListRowLead: View {
  /// What leads the row.
  let lead: ListRow.Lead
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.marksSwapAtOnce) private var swapsAtOnce

  /// The lead, or nothing.
  var body: some View {
    switch lead {
    case .none:
      EmptyView()
    case .tile(let icon):
      SymbolTile(icon: icon, size: .large)
    case .image(let icon):
      Image(decorative: icon.image, scale: 1)
        .resizable()
        .interpolation(.high)
        .frame(width: ListRow.leadWidth(lead), height: ListRow.leadWidth(lead))
        .clipShape(RoundedRectangle(cornerRadius: Radius.extraSmall))
    case .picture:
      AccountPicture(side: ListRow.leadWidth(lead))
    case .mark(let kind):
      AgentMark(kind: kind)
        .frame(width: ListRow.leadWidth(lead), height: ListRow.leadWidth(lead))
    case .letter(let letter):
      Text(letter)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
        .frame(width: ListRow.leadWidth(lead), height: ListRow.leadWidth(lead))
        .background(
          Circle()
            .fill(Color.fillAvatar)
        )
        .accessibilityHidden(true)
    case .check(let state):
      OutcomeMark(kind: StatusLine.Kind(state), side: ListRow.leadWidth(lead))
        .id(state)
        .transition(Motion.transition(Self.landing(state, swapsAtOnce: swapsAtOnce), reduceMotion: reduceMotion))
    case .pending:
      Circle()
        .strokeBorder(Color.strokeControl, lineWidth: Layout.ring)
        .frame(width: ListRow.leadWidth(lead), height: ListRow.leadWidth(lead))
        .accessibilityHidden(true)
    }
  }

  /// How a check's new mark takes the old one's place: popping in from small as it fades in; or, where marks swap at
  /// once, a spinner fading in with no scale, a result popping in from ``Layout/landingScale``, and the old mark only
  /// fading out.
  ///
  /// - Parameters:
  ///   - state: The new mark's state.
  ///   - swapsAtOnce: Whether marks swap at once here.
  /// - Returns: The transition.
  static func landing(_ state: CheckState, swapsAtOnce: Bool) -> AnyTransition {
    let popping = AnyTransition.scale(scale: swapsAtOnce ? Layout.landingScale : Layout.markArrival)
      .combined(with: .opacity)
    guard swapsAtOnce else { return popping }
    return .asymmetric(insertion: state == .busy ? .opacity : popping, removal: .opacity)
  }
}

/// A lead's own measures.
private enum Layout {
  /// 1.2: the empty ring's line.
  static let ring: CGFloat = 1.2
  /// 0.5: how small a check's new mark starts as it pops in.
  static let markArrival: CGFloat = 0.5
  /// 0.6: how small a result starts as it lands where marks swap at once.
  static let landingScale: CGFloat = 0.6
}
