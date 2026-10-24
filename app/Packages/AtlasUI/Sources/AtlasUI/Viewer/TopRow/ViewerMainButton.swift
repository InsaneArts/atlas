import SwiftUI

/// The white capsule in its slot (v3 X5, V12): Take Control, Give Back or Recover…, its title morphing in place in a
/// capsule at least 116 wide, so the row does not move. A refused Take Control shakes it once.
struct ViewerMainButton: View {
  /// The button; nil while the stage offers what to do instead.
  let control: ViewerControlButton?
  /// Take Control's refusals counted.
  let refusals: Int
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The capsule, its title morphing in place.
  var body: some View {
    Button {
      if let control { send(control.event) }
    } label: {
      Text(control?.title ?? "")
        .contentTransition(.interpolate)
        .frame(minWidth: Layout.labelMinWidth)
        .animation(
          Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.reduced),
          value: control?.title
        )
    }
    .buttonStyle(.capsule(size: .small))
    .disabled(control?.isEnabled != true)
    .shakes(refusals)
    .help(control?.help ?? "")
    .accessibilityLabel(control?.title ?? "")
    .actionSlot(control != nil)
  }
}

/// The white capsule's own measure.
private enum Layout {
  /// 18: a capsule's inset each side of its title (``CapsuleButtonStyle``'s for a capsule that hugs its title).
  static let capsuleInset: CGFloat = 18
  /// 80: its title's least width, the capsule's 116 less its inset each side (v3 `min-width: 116px`).
  static let labelMinWidth = ViewerLayout.mainButtonMinWidth - capsuleInset * 2
}

#if DEBUG
  #Preview("Take Control and Give Back") {
    HStack(spacing: Spacing.s) {
      ViewerMainButton(control: PreviewStates.viewerWorking.top.actions.control, refusals: 0) { _ in }
      ViewerMainButton(control: PreviewStates.viewerControl.top.actions.control, refusals: 0) { _ in }
    }
    .padding()
    .sampleSurface()
  }
#endif
