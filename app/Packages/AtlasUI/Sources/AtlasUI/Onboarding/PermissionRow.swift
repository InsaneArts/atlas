import SwiftUI

/// One permission: whether it is on, or, for the one allowed next, Allow… to open its switch in System Settings.
struct PermissionRow: View {
  /// The permission.
  let permission: OnboardingPermissionItem
  /// Its tile's symbol.
  let symbol: String
  /// What it lets agents do.
  let detail: String
  /// Receives Allow…, pressed by Return, for the permission to allow next; nil where the row only shows whether it is
  /// on.
  var send: (@MainActor (OnboardingEvent) -> Void)?

  /// The permission's tile, name and use, then whether it is on, or its Allow… button.
  var body: some View {
    // A permission's name is long on some macOS versions (Device Control and Data Access): it wraps.
    IconRow(icon: .symbol(symbol), title: permission.title, detail: detail, wraps: true) {
      if let send = allow {
        Button(permission.allowTitle) { send(.openSettings(id: permission.id)) }
          .buttonStyle(.capsule(.primary, size: .small))
          .keyboardShortcut(.defaultAction)
      } else {
        Text(permission.status)
          .font(Typography.font(.callout, weight: .semibold))
          .foregroundStyle(permission.isOn ? Color.textSuccess : Color.textTertiary)
      }
    }
    // Read as one line, unless its button needs pressing on its own.
    .accessibilityElement(children: allow == nil ? .combine : .contain)
  }

  /// Where Allow… goes, while the row shows it.
  private var allow: (@MainActor (OnboardingEvent) -> Void)? {
    permission.canAllow && !permission.isOn ? send : nil
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
      ) { _ in }
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
