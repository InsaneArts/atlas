import SwiftUI

/// The compact panel beside System Settings while one permission is allowed: Atlas's icon to drag into the list, the
/// switch for Atlas listed already, both permissions, and what it waits for.
struct PermissionPanel<DragIcon: View>: View {
  /// The permission allowed now, and both permissions.
  let state: OnboardingPermissionPanelState
  /// Atlas's icon, which the person drags into System Settings: the app hands in its own, as no drag starts here.
  let dragIcon: DragIcon
  /// Receives Back.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The icon, what to do, both permissions, the pill and Back.
  var body: some View {
    VStack(spacing: Spacing.xl) {
      GroupLabel(text: state.label)
      dragIcon
        .frame(width: OnboardingLayout.dragIcon, height: OnboardingLayout.dragIcon)
      VStack(spacing: Spacing.s) {
        Headline(title: state.instruction, size: .compact)
        Note(text: state.alreadyListed)
      }
      CardGroup {
        PermissionRow(permission: state.accessibility, symbol: "cursorarrow", detail: "Click, type, read buttons")
        SeparatorLine()
        PermissionRow(permission: state.screenRecording, symbol: "rectangle.dashed", detail: "Take screenshots")
      }
      Spacer(minLength: Spacing.m)
      ProgressPill(text: state.pill, done: state.isOn)
      Button("Back") { send(.closePanel) }
        .buttonStyle(.capsule(.plain))
        .keyboardShortcut(.cancelAction)
    }
    .padding(.horizontal, Spacing.section)
    .padding(.top, OnboardingLayout.panelTop)
    .padding(.bottom, OnboardingLayout.panelBottom)
  }
}

#if DEBUG
  #Preview("Permission panel") {
    PermissionPanel(
      state: PreviewStates.onboardingPermissionPanelState,
      dragIcon: AppIcon(size: OnboardingLayout.dragIcon)
    ) {
      _ in
    }
    .onboardingSample(OnboardingLayout.side)
  }
#endif
