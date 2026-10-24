import SwiftUI

/// One permission, a row of the permissions screen's group (v3 D6): its tile, its name as this macOS names it, what it
/// lets agents do, and where it stands at the row's end: Allow… for the one to allow next, Allowed, Checking… or Off.
struct PermissionRow: View {
  /// The permission.
  let permission: OnboardingPermissionItem
  /// Its tile's symbol.
  let symbol: SymbolName
  /// What it lets agents do.
  let detail: String
  /// Receives Allow…, pressed by Return, for the permission to allow next.
  let send: @MainActor (OnboardingEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The tile, the name over its use, and where it stands, which lands in place as it changes.
  var body: some View {
    ListRowBody(
      permission.title,
      detail: detail,
      lead: .tile(.symbol(symbol.rawValue)),
      // Read as one line, unless its button needs pressing on its own.
      accessibilityChildren: standing.isPressable ? .contain : .combine
    ) {
      PermissionRowTrailing(standing: standing) { send(.openSettings(id: permission.id)) }
    }
    .animation(Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced), value: standing)
  }

  /// Where the permission stands, Allow… offered while it is the next to allow.
  private var standing: OnboardingPermissionStanding {
    .of(permission, offersAllow: true)
  }
}

extension OnboardingPermissionStanding {
  /// Whether its row ends in a button.
  fileprivate var isPressable: Bool {
    if case .allow = self { return true }
    return false
  }
}

#if DEBUG
  #Preview("Each standing") {
    GroupedList(PreviewStates.onboardingPermissionSamples) { permission in
      PermissionRow(
        permission: permission,
        symbol: .click,
        detail: String("So agents can click, type and read buttons")
      ) { _ in }
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
