import SwiftUI

/// What leads a ``ListRow``: a tile, an app's icon, the account picture, a letter disc, a check's mark or the
/// empty ring of a check not run yet, each in the width ``ListRow/leadWidth(_:)`` gives it. A check's mark is
/// replaced as its state changes, popping in where its owner animates the change.
struct ListRowLead: View {
  /// What leads the row.
  let lead: ListRow.Lead
  @Environment(\.motionReduced) private var reduceMotion
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
        .clipShape(RoundedRectangle(cornerRadius: Radius.extraSmall, style: .continuous))
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
        .strokeBorder(Color.strokeControl, lineWidth: ListRowLeadLayout.ring)
        .frame(width: ListRow.leadWidth(lead), height: ListRow.leadWidth(lead))
        .accessibilityHidden(true)
    }
  }

  /// How a check's new mark takes the old one's place: popping in from small as it fades in; or, where marks swap at
  /// once, a spinner fading in with no scale, a result popping in from ``ListRowLeadLayout/landingScale``, and the old mark only
  /// fading out.
  ///
  /// - Parameters:
  ///   - state: The new mark's state.
  ///   - swapsAtOnce: Whether marks swap at once here.
  /// - Returns: The transition.
  static func landing(_ state: CheckState, swapsAtOnce: Bool) -> AnyTransition {
    let popping = AnyTransition.scale(
      scale: swapsAtOnce ? ListRowLeadLayout.landingScale : ListRowLeadLayout.markArrival
    )
    .combined(with: .opacity)
    guard swapsAtOnce else { return popping }
    return .asymmetric(insertion: state == .busy ? .opacity : popping, removal: .opacity)
  }
}
