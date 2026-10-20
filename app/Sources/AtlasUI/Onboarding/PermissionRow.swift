import SwiftUI

/// One permission and whether it is on.
struct PermissionRow: View {
  /// The permission.
  let permission: OnboardingPermissionItem
  /// Its tile's symbol.
  let symbol: String
  /// What it lets agents do.
  let detail: String

  /// The permission's tile, name and use, then whether it is on.
  var body: some View {
    IconRow(icon: .symbol(symbol), title: permission.title, detail: detail) {
      Text(permission.status)
        .font(Typography.font(.callout, weight: .semibold))
        .foregroundStyle(permission.isOn ? Color.textSuccess : Color.textTertiary)
    }
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("On and off") {
    CardGroup {
      PermissionRow(
        permission: PreviewStates.onboardingPermissionsScreen.accessibility,
        symbol: "cursorarrow",
        detail: String("So agents can click, type and read buttons")
      )
      PermissionRow(
        permission: PreviewStates.onboardingPermissionsScreen.screenRecording,
        symbol: "rectangle.dashed",
        detail: String("So agents can take screenshots of this desktop")
      )
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
