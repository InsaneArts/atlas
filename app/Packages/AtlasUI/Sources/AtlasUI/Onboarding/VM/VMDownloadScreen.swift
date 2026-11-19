import SwiftUI

/// macOS coming down from Apple for the VM: the meter and how much of the whole is down, why the add is held when it
/// is, then the licence's small print. Continue waits in the footer until the VM is made, or Go On or Try Again takes
/// its place once the add is held.
struct VMDownloadScreen: View {
  /// The download.
  let state: OnboardingVMDownloadScreenState

  /// The header, the meter and its amount, the status line when held, and the small print.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(
        label: "VM",
        title: OnboardingScreen.vmDownloadTitle,
        tail: OnboardingScreen.vmDownloadTail,
        lede:
          "Atlas downloads macOS from Apple and makes a virtual Mac with it. You can close this window; the download goes on."
      )
      VStack(spacing: Spacing.s) {
        MeterBar(fraction: state.fraction, isHeld: state.isHeld)
        Text(verbatim: state.amount ?? " ")
          .textStyle(.callout)
          .monospacedDigit()
          .foregroundStyle(Color.textSecondary)
      }
      .accessibilityElement(children: .ignore)
      .accessibilityLabel("Download")
      .accessibilityValue(state.amount ?? "")
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      if let problem = state.problem {
        StatusSlot(line: StatusLine(kind: .problem, text: problem))
      }
      Note(text: "For developing and testing software, as macOS’s licence allows. The VM uses up to 128 GB of disk.")
        .padding(.top, OnboardingLayout.Rhythm.paragraph)
    }
  }
}

#if DEBUG
  #Preview("Download") {
    VMDownloadScreen(state: PreviewStates.onboardingVMDownloadScreen)
      .onboardingSample()
  }

  #Preview("Download, stopped") {
    VMDownloadScreen(state: PreviewStates.onboardingVMStoppedScreen)
      .onboardingSample()
  }
#endif
