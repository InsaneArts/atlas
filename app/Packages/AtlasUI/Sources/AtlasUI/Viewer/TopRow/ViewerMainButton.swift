import SwiftUI

/// The white capsule in its slot: Take Control, Give Back, Connecting… or Recover…, as wide as the widest of them
/// whichever it shows, so it never changes width because of its own state; a new title swaps in place by opacity. A
/// refused Take Control shakes it once.
struct ViewerMainButton: View {
  /// The button; nil while the stage offers what to do instead.
  let control: ViewerControlButton?
  /// Take Control's refusals counted.
  let refusals: Int
  /// Receives the press.
  let send: @MainActor (ViewerEvent) -> Void

  /// The capsule, its title swapping in place over the room the widest takes.
  var body: some View {
    Button {
      if let control { send(control.event) }
    } label: {
      ZStack {
        ForEach(control?.titles ?? [], id: \.self) { title in
          Text(title)
            .hidden()
        }
        Text(control?.title ?? "")
          .swapsText(on: control?.title)
      }
      .frame(minWidth: Layout.labelMinWidth)
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
  /// 80: its title's least width, the capsule's 116 less its inset each side.
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
