import SwiftUI

/// The narrow window beside System Settings while one permission is allowed: which permission, Atlas's icon in its well
/// to drag into the list, what to do there and the way for Atlas listed already, both permissions as two steps on a
/// rail, only the one guided now lit, and one status line in the same words as the permissions screen: Waiting for
/// you… with no spinner, as the person acts, then Allowed. Back is the footer's, at its bottom left.
struct PermissionPanel<DragIcon: View>: View {
  /// The permission allowed now, and both permissions.
  let state: OnboardingPermissionPanelState
  /// Atlas's icon, which the person drags into System Settings: the app hands in its own, as no drag starts here.
  let dragIcon: DragIcon

  /// The label, the icon's well and the title, the two steps, then the status line.
  var body: some View {
    PageColumn(width: OnboardingLayout.sideColumn, top: OnboardingLayout.contentTop + Spacing.l) {
      OnboardingHeader(
        label: state.label,
        title: state.instruction,
        lede: state.alreadyListed,
        titleSize: .compact
      ) {
        DragWell(icon: dragIcon)
      }
      StepList(steps: Self.steps(state))
        .padding(.top, OnboardingLayout.Rhythm.paragraph)
      StatusSlot { status }
    }
  }

  /// The status line for where the permission guided now stands: Allowed with its check, Checking… with a spinner
  /// while Atlas reads it, else the waiting words alone, with no mark, as the person acts, not Atlas.
  @ViewBuilder private var status: some View {
    switch state.status {
    case .allowed(let word):
      StatusLine(kind: .done, text: word)
    case .checking(let word):
      StatusLine(kind: .working, text: word)
    case .waiting(let word), .next(let word), .turnedOff(let word):
      Text(word)
        .textStyle(.callout)
        .foregroundStyle(Color.textSecondary)
        .swapsText(on: word)
    case .yourTurn:
      EmptyView()
    }
  }

  /// Accessibility, then Screen Recording, each under its name as this macOS names it: done once on, lit only while
  /// it is the one guided now, else waiting, and reading Checking… in place of what it lets agents do while it cannot
  /// be read. The header, the steps and the status change together as the panel moves on.
  ///
  /// - Parameter state: The panel.
  /// - Returns: The two steps.
  static func steps(_ state: OnboardingPermissionPanelState) -> [StepList.Step] {
    [
      step(state.accessibility, detail: "Click, type, read buttons"),
      step(state.screenRecording, detail: "Take screenshots"),
    ]
  }

  /// One permission as a step.
  ///
  /// - Parameters:
  ///   - permission: The permission.
  ///   - detail: What it lets agents do, in a few words.
  /// - Returns: The step.
  private static func step(_ permission: OnboardingPermissionItem, detail: String) -> StepList.Step {
    let state: StepState =
      if permission.isOn { .done } else if permission.isCurrent { .current } else { .waiting }
    guard case .checking(let word) = permission.standing else {
      return StepList.Step(title: permission.title, detail: detail, state: state)
    }
    return StepList.Step(title: permission.title, detail: word, state: state)
  }
}

#if DEBUG
  #Preview("Permission panel") {
    PermissionPanel(
      state: PreviewStates.onboardingPermissionPanelState,
      dragIcon: AppIcon(tile: OnboardingLayout.dragTile)
    )
    .onboardingSample(OnboardingLayout.side)
  }
#endif
