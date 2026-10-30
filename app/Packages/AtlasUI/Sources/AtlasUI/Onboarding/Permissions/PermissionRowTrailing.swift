import SwiftUI

/// Where a permission stands, at its row's end, in a place of one width for every word: a green check that pops in
/// beside Allowed, a spinner beside Checking… once a moment has passed, a quiet Next or Waiting for you…, an orange
/// Turned off, or nothing while the footer offers it. One standing gives way to the next where it sits: the new one
/// pops in from 60% as the old one shrinks out, and under Reduce Motion they crossfade.
struct PermissionRowTrailing: View {
  /// Where the permission stands.
  let standing: OnboardingPermissionStanding
  @Environment(\.motionReduced) private var reduceMotion

  /// The mark and word, or the word, landing in place of the last.
  var body: some View {
    ZStack(alignment: .trailing) {
      word
        .id(standing)
        .transition(Motion.transition(Self.landing, reduceMotion: reduceMotion))
    }
    .frame(width: OnboardingLayout.permissionStanding, alignment: .trailing)
  }

  /// The word for the standing now.
  @ViewBuilder private var word: some View {
    switch standing {
    case .allowed(let word):
      ListRowTrailing(trailing: .check(.done, word), action: nil)
    case .checking(let word):
      // Atlas checks, so it waits a moment before it shows, never flashing for a reading that comes back at once.
      ListRowTrailing(trailing: .check(.busy, word), action: nil)
        .appearsAsNote(holds: true)
    case .waiting(let word), .next(let word):
      ListRowTrailing(trailing: .text(word), action: nil)
    case .turnedOff(let word):
      ListRowTrailing(trailing: .text(word, tone: .problem), action: nil)
    case .yourTurn:
      EmptyView()
    }
  }

  /// How one standing gives way to the next: the old one shrinks out as it fades, the new one pops in.
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

/// The landing mark's measures, as ``OutcomeMark`` lands.
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
      PermissionRowTrailing(standing: isOn ? .allowed(String("Allowed")) : .next(String("Next")))
    }
  }
#endif
