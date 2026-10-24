import SwiftUI

/// Where a permission stands, at its row's end: the small glass Allow…, which Return presses (Q-D3); a green check
/// that pops in beside Allowed; a spinner beside Checking…; or a quiet Off. One standing gives way to the next where it
/// sits: the new one pops in from 60% as the old one shrinks out, and under Reduce Motion they crossfade.
struct PermissionRowTrailing: View {
  /// Where the permission stands.
  let standing: OnboardingPermissionStanding
  /// Opens its list in System Settings.
  let allow: @MainActor () -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The button, the mark and word, or the word, landing in place of the last.
  var body: some View {
    ZStack(alignment: .trailing) {
      control
        .id(standing)
        .transition(Motion.transition(Self.landing, reduceMotion: reduceMotion))
    }
  }

  /// The control for the standing now.
  @ViewBuilder private var control: some View {
    switch standing {
    case .allow(let title):
      Button(title, action: allow)
        .buttonStyle(.capsule(.glass, size: .small))
        .keyboardShortcut(.defaultAction)
    case .allowed(let word):
      ListRowTrailing(trailing: .check(.done, word), action: nil)
    case .checking(let word):
      ListRowTrailing(trailing: .check(.busy, word), action: nil)
    case .off(let word):
      ListRowTrailing(trailing: .text(word), action: nil)
    }
  }

  /// How one standing gives way to the next (v3 `land`): the old one shrinks out as it fades, the new one pops in.
  private static var landing: AnyTransition {
    .asymmetric(
      insertion: .scale(scale: Layout.landingScale, anchor: .trailing)
        .combined(with: .opacity)
        .animation(Motion.pop),
      removal: .scale(scale: Layout.landingScale, anchor: .trailing)
        .combined(with: .opacity)
        .animation(Motion.leave)
    )
  }
}

/// v3's landing mark (`land`), as ``OutcomeMark`` lands.
private enum Layout {
  /// 0.6: the size a standing shrinks to as it leaves, and pops in from.
  static let landingScale: CGFloat = 0.6
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var isOn = false
    InteractivePreview {
      Toggle(String("Allowed"), isOn: $isOn.animation(Motion.state))
    } content: {
      PermissionRowTrailing(standing: isOn ? .allowed(String("Allowed")) : .allow(String("Allow…"))) {}
    }
  }
#endif
