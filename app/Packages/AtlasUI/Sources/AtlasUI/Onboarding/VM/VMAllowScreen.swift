import SwiftUI

/// The person allows Atlas in the VM's own window: the two permissions as steps done there, so this screen has no
/// button of its own, why the add is held when it is, and where the administrator's name and password are. Continue
/// waits in the footer until the VM is set up.
struct VMAllowScreen: View {
  /// The two permissions.
  let state: OnboardingVMAllowScreenState

  /// The header, the two steps, the status line when held, and the note.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(
        label: "VM",
        title: OnboardingScreen.vmAllowTitle,
        tail: OnboardingScreen.vmAllowTail,
        lede:
          "The VM’s window is open. In it, allow Atlas when macOS asks: Screen & System Audio Recording, then Accessibility."
      )
      StepList(steps: Self.steps(state))
        .padding(.top, OnboardingLayout.Rhythm.paragraph)
      if let problem = state.problem {
        StatusSlot(line: StatusLine(kind: .problem, text: problem))
      }
      Note(text: "If macOS asks for an administrator, the VM’s window shows the name and password.")
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }

  /// Screen & System Audio Recording, then Accessibility, each where the guest says it stands.
  ///
  /// - Parameter state: The allow screen.
  /// - Returns: The two steps.
  static func steps(_ state: OnboardingVMAllowScreenState) -> [StepList.Step] {
    let titles = ["Screen & System Audio Recording", "Accessibility"]
    return zip(titles, state.steps)
      .map { title, step in StepList.Step(title: title, detail: "", state: step) }
  }
}

#if DEBUG
  #Preview("Allow") {
    VMAllowScreen(state: PreviewStates.onboardingVMAllowScreen)
      .onboardingSample()
  }
#endif
