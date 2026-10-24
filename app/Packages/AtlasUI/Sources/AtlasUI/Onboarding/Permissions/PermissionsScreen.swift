import SwiftUI

/// In the helper: Accessibility and Screen Recording, allowed one after the other in System Settings, in one group
/// (v3 D6). Each row keeps its own Allow…, so the footer holds no main action here, and Return allows the next one
/// (Q-D3).
struct PermissionsScreen: View {
  /// The two permissions.
  let state: OnboardingPermissionsScreenState
  /// Receives the permission to allow next.
  let send: @MainActor (OnboardingEvent) -> Void

  /// Why Atlas asks, the two permissions, and how Atlas notices them.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
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
          detail: Self.detail(permission, in: state),
          send: send
        )
      }
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
      Note(text: "Allow opens the list in System Settings. Atlas notices each one on its own.")
        .padding(.top, OnboardingLayout.Rhythm.status)
        .pagePiece(.status)
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
    PermissionsScreen(state: PreviewStates.onboardingPermissionsScreen) { _ in }
      .onboardingSample()
  }
#endif
