import SwiftUI

/// A slot's control part of the way into its place (``ActionSlot``), carried by the row's own move, so it keeps the
/// same beats whatever animation moves the row and on every macOS: unseen while its neighbours make room for it, then
/// fading in and growing from its leading edge over the last of the move; going, it fades and shrinks over the first
/// of the move, before a neighbour reaches its place.
struct ActionSlotBeat: ViewModifier, Animatable {
  /// 0.8: the share of the move its neighbours make before the control shows, and the share left once it has gone.
  static let unseen = 0.8
  /// 0.94: the control's size as it starts to show, close to whole, so it reads as arriving rather than popping.
  static let arrivingScale = 0.94
  /// How far into its place the control is, from 0, gone, to 1, in place.
  var progress: Double

  /// The progress, which SwiftUI moves along the row's animation.
  var animatableData: Double {
    get { progress }
    set { progress = newValue }
  }

  /// The control, as much of it showing as its progress allows.
  ///
  /// - Parameter content: The control.
  /// - Returns: The control, faded and scaled.
  func body(content: Content) -> some View {
    let shown = Self.shown(at: progress)
    content
      .opacity(shown)
      .scaleEffect(Self.arrivingScale + (1 - Self.arrivingScale) * shown, anchor: .leading)
  }

  /// How much of the control shows `progress` of the way into its place: none until ``unseen`` of the way, then
  /// evenly more until all of it at the end.
  ///
  /// - Parameter progress: How far into its place it is, from 0 to 1.
  /// - Returns: How much shows, from 0 to 1.
  static func shown(at progress: Double) -> Double {
    min(max((progress - unseen) / (1 - unseen), .zero), 1)
  }
}
