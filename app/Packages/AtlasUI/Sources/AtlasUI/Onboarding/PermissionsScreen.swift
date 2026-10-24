import SwiftUI

/// In the helper: Accessibility and Screen Recording, allowed one after the other in System Settings, the next one
/// with its Allow… button.
struct PermissionsScreen: View {
  /// The two permissions.
  let state: OnboardingPermissionsScreenState
  /// Receives the permission to allow next.
  let send: @MainActor (OnboardingEvent) -> Void

  /// Why Atlas asks, and the two permissions; Return allows the next one.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.listColumn) {
      GroupLabel(text: "Permissions")
      Headline(title: OnboardingScreen.permissionsTitle, tail: OnboardingScreen.permissionsTail)
      Lede(
        text:
          "You allow Atlas twice. Atlas uses them only in this account, never in your own."
      )
      CardGroup {
        PermissionRow(
          permission: state.accessibility,
          symbol: "cursorarrow",
          detail: "So agents can click, type and read buttons",
          send: send
        )
        SeparatorLine()
        PermissionRow(
          permission: state.screenRecording,
          symbol: "rectangle.dashed",
          detail: "So agents can take screenshots of this desktop",
          send: send
        )
      }
      .padding(.top, Spacing.xs)
      Note(text: "Allow opens the list in System Settings. Atlas notices each one on its own.")
    }
  }
}

#if DEBUG
  #Preview("Helper permissions") {
    PermissionsScreen(state: PreviewStates.onboardingPermissionsScreen) { _ in }
      .onboardingSample()
  }
#endif
