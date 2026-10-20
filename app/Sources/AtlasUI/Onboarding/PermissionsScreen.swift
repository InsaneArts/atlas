import SwiftUI

/// In the helper: Accessibility and Screen Recording, and the button for the next one still off.
struct PermissionsScreen: View {
  /// The two permissions.
  let state: OnboardingPermissionsScreenState
  /// Receives the permission to allow next.
  let send: @MainActor (OnboardingEvent) -> Void

  /// Why Atlas asks, the two permissions, and the next one's button.
  var body: some View {
    ScreenColumn(width: OnboardingLayout.listColumn) {
      Eyebrow(text: "Permissions")
      Headline(title: "Let Atlas see and use", tail: "this screen.")
      Lede(
        text:
          "macOS asks you to allow Atlas twice, in this account only. Your own account is never touched."
      )
      CardGroup {
        PermissionRow(
          permission: state.accessibility,
          symbol: "cursorarrow",
          detail: "So agents can click, type and read buttons"
        )
        SeparatorLine()
        PermissionRow(
          permission: state.screenRecording,
          symbol: "rectangle.dashed",
          detail: "So agents can take screenshots of this desktop"
        )
      }
      .padding(.top, Spacing.xs)
      if let next = state.next {
        Button(next.allowTitle) { send(.openSettings(id: next.id)) }
          .buttonStyle(.capsule(size: .large))
          .keyboardShortcut(.defaultAction)
          .padding(.top, Spacing.s)
      }
      Note(
        text:
          "Atlas notices each switch on its own. If macOS offers to quit and reopen Atlas, either choice is fine."
      )
    }
  }
}

#if DEBUG
  #Preview("Helper permissions") {
    PermissionsScreen(state: PreviewStates.onboardingPermissionsScreen) { _ in }
      .onboardingSample()
  }
#endif
