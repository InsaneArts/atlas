import SwiftUI

/// Atlas making the VM, in four steps joined by a rail, the one running lit, each a title alone, then why the add is
/// held when it is. Continue waits in the footer until the VM is made, or Go On or Try Again takes its place.
struct VMInstallScreen: View {
  /// The four steps.
  let state: OnboardingVMInstallScreenState

  /// The header, the steps and the status line when held.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(label: "VM", title: OnboardingScreen.vmInstallTitle, tail: OnboardingScreen.vmInstallTail)
      StepList(steps: Self.steps(state))
        .padding(.top, OnboardingLayout.Rhythm.paragraph)
      if let problem = state.problem {
        StatusSlot(line: StatusLine(kind: .problem, text: problem))
      }
    }
  }

  /// Install macOS, start the VM, set up Atlas inside, connect to Atlas, each where the add puts it.
  ///
  /// - Parameter state: The install.
  /// - Returns: The four steps.
  static func steps(_ state: OnboardingVMInstallScreenState) -> [StepList.Step] {
    let titles = ["Install macOS", "Start the VM", "Set up Atlas inside", "Connect to Atlas"]
    return zip(titles, state.steps)
      .map { title, step in StepList.Step(title: title, detail: "", state: step) }
  }
}

#if DEBUG
  #Preview("Install") {
    VMInstallScreen(state: PreviewStates.onboardingVMInstallScreen)
      .onboardingSample()
  }

  #Preview("Install, failed") {
    VMInstallScreen(state: PreviewStates.onboardingVMFailedScreen)
      .onboardingSample()
  }
#endif
