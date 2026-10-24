import SwiftUI

/// The dot after Activity's word while it holds actions the person has not seen (v3 V2): blue, or orange once one of
/// them failed. It pops in (``Motion/pop``; a fade under Reduce Motion) and goes as the tab is chosen.
struct ViewerUnseenDot: View {
  /// What Activity holds unseen.
  let unseen: ViewerUnseen
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The dot, popping in while something is unseen.
  var body: some View {
    Circle()
      .fill(unseen == .failed ? Color.attention : Color.accent)
      .frame(width: IconSize.xxs, height: IconSize.xxs)
      .scaleEffect(Self.scale(unseen, reduceMotion: reduceMotion))
      .opacity(unseen == .none ? 0 : 1)
      .animation(Motion.animation(Motion.pop, reduceMotion: reduceMotion, reduced: Motion.reduced), value: unseen)
      .accessibilityHidden(true)
  }

  /// How large the dot is: whole while it shows, nothing while it does not, so it pops in; whole either way under
  /// Reduce Motion, where it only fades.
  ///
  /// - Parameters:
  ///   - unseen: What Activity holds unseen.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  /// - Returns: The dot's scale.
  nonisolated static func scale(_ unseen: ViewerUnseen, reduceMotion: Bool) -> CGFloat {
    guard unseen == .none, !reduceMotion else { return 1 }
    return .zero
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var unseen = ViewerUnseen.new
    InteractivePreview {
      Picker(String("Unseen"), selection: $unseen) {
        Text(verbatim: "None")
          .tag(ViewerUnseen.none)
        Text(verbatim: "New")
          .tag(ViewerUnseen.new)
        Text(verbatim: "Failed")
          .tag(ViewerUnseen.failed)
      }
    } content: {
      ViewerUnseenDot(unseen: unseen)
    }
  }
#endif
