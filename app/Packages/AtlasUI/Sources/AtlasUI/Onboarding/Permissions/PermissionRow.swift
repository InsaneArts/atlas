import SwiftUI

/// One permission, a row of the permissions screen's group: its tile, its name as this macOS names it, what it lets
/// agents do, and where it stands at the row's end. A permission whose turn has not come yet is dimmed. The row only
/// shows: the footer's button allows the next one.
struct PermissionRow: View {
  /// The permission.
  let permission: OnboardingPermissionItem
  /// Its tile's symbol.
  let symbol: SymbolName
  /// What it lets agents do.
  let detail: String
  @Environment(\.motionReduced) private var reduceMotion

  /// The tile, the name over its use, and where it stands, which lands in place as it changes, read as one line.
  var body: some View {
    ListRowBody(
      permission.title,
      detail: detail,
      lead: .tile(.symbol(symbol.rawValue)),
      accessibilityChildren: .combine
    ) {
      PermissionRowTrailing(standing: permission.standing)
    }
    .opacity(permission.standing.dimsRow ? Opacity.notYet : 1)
    .animation(
      Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced),
      value: permission.standing
    )
  }
}

#if DEBUG
  #Preview("Each standing") {
    GroupedList(PreviewStates.onboardingPermissionSamples) { permission in
      PermissionRow(
        permission: permission,
        symbol: .click,
        detail: String("So agents can click, type and read buttons")
      )
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
