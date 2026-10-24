import SwiftUI

/// The compact panel beside System Settings while one permission is allowed: Atlas's icon to drag into the list, the
/// switch for Atlas listed already, both permissions, and what it waits for. Back is the footer's, at its bottom left.
struct PermissionPanel<DragIcon: View>: View {
  /// The permission allowed now, and both permissions.
  let state: OnboardingPermissionPanelState
  /// Atlas's icon, which the person drags into System Settings: the app hands in its own, as no drag starts here.
  let dragIcon: DragIcon

  /// The icon, what to do, both permissions and the pill.
  var body: some View {
    VStack(spacing: Spacing.mPlus) {
      GroupLabel(text: state.label)
      dragIcon
        .frame(width: OnboardingLayout.dragIcon, height: OnboardingLayout.dragIcon)
      VStack(spacing: Spacing.xsPlus) {
        Headline(title: state.instruction, size: .compact)
        Note(text: state.alreadyListed)
      }
      CardGroup {
        PermissionRow(permission: state.accessibility, symbol: "cursorarrow", detail: "Click, type, read buttons")
        SeparatorLine()
        PermissionRow(permission: state.screenRecording, symbol: "rectangle.dashed", detail: "Take screenshots")
      }
      Spacer(minLength: Spacing.s)
      ProgressPill(text: state.pill, done: state.isOn)
    }
    .padding(.horizontal, Spacing.xxl)
    .padding(.top, OnboardingLayout.panelTop)
    .padding(.bottom, OnboardingLayout.panelBottom)
  }
}

#if DEBUG
  #Preview("Permission panel") {
    PermissionPanel(
      state: PreviewStates.onboardingPermissionPanelState,
      dragIcon: AppIcon(size: OnboardingLayout.dragIcon)
    )
    .onboardingSample(OnboardingLayout.side)
  }
#endif
