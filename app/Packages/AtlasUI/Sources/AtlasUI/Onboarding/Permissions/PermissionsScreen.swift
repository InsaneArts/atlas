import SwiftUI

/// In the helper: Accessibility and Screen Recording, allowed one after the other in System Settings, in one group.
/// The rows only say where each stands; the footer's button allows the next one, and Return presses it.
struct PermissionsScreen: View {
  /// The two permissions.
  let state: OnboardingPermissionsScreenState

  /// Why Atlas asks, the two permissions, and how Atlas notices them.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(
        label: "Permissions",
        title: OnboardingScreen.permissionsTitle,
        tail: OnboardingScreen.permissionsTail,
        lede: "You allow Atlas twice. Atlas uses them only in this account, never in your own."
      )
      GroupedList([state.accessibility, state.screenRecording]) { permission in
        PermissionRow(
          permission: permission,
          symbol: Self.symbol(permission, in: state),
          detail: Self.detail(permission, in: state)
        )
      }
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      Note(text: "Each button opens the list in System Settings. Atlas notices each one on its own.")
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }

  /// A permission's tile: a click for Accessibility, the area a screenshot takes for Screen Recording.
  ///
  /// - Parameters:
  ///   - permission: The permission.
  ///   - state: The screen, which says which one is Accessibility.
  /// - Returns: The tile's symbol.
  static func symbol(_ permission: OnboardingPermissionItem, in state: OnboardingPermissionsScreenState) -> SymbolName {
    permission.id == state.accessibility.id ? .click : .screenArea
  }

  /// What a permission lets agents do.
  ///
  /// - Parameters:
  ///   - permission: The permission.
  ///   - state: The screen, which says which one is Accessibility.
  /// - Returns: Its use, in words.
  static func detail(_ permission: OnboardingPermissionItem, in state: OnboardingPermissionsScreenState) -> String {
    permission.id == state.accessibility.id
      ? "So agents can click, type and read buttons" : "So agents can take screenshots of this desktop"
  }
}

#if DEBUG
  #Preview("Helper permissions") {
    PermissionsScreen(state: PreviewStates.onboardingPermissionsScreen)
      .onboardingSample()
  }
#endif
