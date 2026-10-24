import SwiftUI

/// What ``PageSlide`` does to a page at one phase of its transition: fades it, and moves it sideways the way the pages
/// go, read from ``SwiftUICore/EnvironmentValues/pageDirection`` as the transition plays.
struct PageSlideEffect: ViewModifier {
  /// The phase of the page's transition.
  let phase: TransitionPhase
  @Environment(\.pageDirection) private var direction

  /// The page, faded and moved for its phase.
  ///
  /// - Parameter content: The page.
  /// - Returns: The page at its phase.
  func body(content: Content) -> some View {
    content
      .opacity(phase.isIdentity ? 1 : 0)
      .offset(x: Self.offset(phase, direction: direction))
  }

  /// How far sideways a page sits at a phase of its transition: an arriving page starts 32 points the way the pages
  /// come from, a leaving page ends 24 points the way they go (v3 S2, X7), and a page in place sits still.
  ///
  /// - Parameters:
  ///   - phase: The phase of the page's transition.
  ///   - direction: Which way the pages move.
  /// - Returns: The page's sideways offset in points: positive to the right.
  nonisolated static func offset(_ phase: TransitionPhase, direction: PageDirection) -> CGFloat {
    let way: CGFloat = direction == .forward ? 1 : -1
    switch phase {
    case .willAppear: return way * Layout.entry
    case .didDisappear: return -way * Layout.exit
    case .identity: return .zero
    }
  }
}

/// v3's X7 distances.
private enum Layout {
  /// 32: how far an arriving page starts from its place.
  static let entry: CGFloat = 32
  /// 24: how far a leaving page goes before it is gone.
  static let exit: CGFloat = 24
}
