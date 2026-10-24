#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// The setup window's design renders. Debug builds only, since the samples are.
  enum OnboardingRender {
    /// Every setup window sample drawn for checking against the design, by file name, in the order the person
    /// meets them.
    static let renders: [(file: String, state: OnboardingState)] = [
      ("00-loading", PreviewStates.onboardingLoading),
      ("00-loading-failed", PreviewStates.onboardingLoadingFailed),
      ("01-welcome", PreviewStates.onboardingWelcome),
      ("02-choose", PreviewStates.onboardingChoose),
      ("02-choose-many", PreviewStates.onboardingChooseMany),
      ("02-choose-long", PreviewStates.onboardingChooseLong),
      ("02-choose-problem", PreviewStates.onboardingChooseProblem),
      ("02-choose-only", PreviewStates.onboardingChooseOnly),
      ("03-create", PreviewStates.onboardingCreate),
      ("03-create-working", PreviewStates.onboardingCreateWorking),
      ("03-create-bad-name", PreviewStates.onboardingCreateBadName),
      ("04-switch", PreviewStates.onboardingSwitch),
      ("04-switch-open-atlas", PreviewStates.onboardingSwitchOpenAtlas),
      ("04-switch-long", PreviewStates.onboardingSwitchLong),
      ("05-finishing", PreviewStates.onboardingFinishing),
      ("05-finishing-passed", PreviewStates.onboardingFinishingPassed),
      ("05-finishing-failed", PreviewStates.onboardingFinishingFailed),
      ("06-agents", PreviewStates.onboardingAgents),
      ("06-agents-not-checked", PreviewStates.onboardingAgentsNotChecked),
      ("07-done", PreviewStates.onboardingDone),
      ("08-helper-allow", PreviewStates.onboardingAllow),
      ("08-helper-allow-long", PreviewStates.onboardingAllowLong),
      ("09-helper-permissions", PreviewStates.onboardingPermissions),
      ("10-helper-permission-panel", PreviewStates.onboardingPermissionPanel),
      ("11-helper-done", PreviewStates.onboardingHelperDone),
      ("12-blocked", PreviewStates.onboardingBlocked),
    ]

    /// Draws every setup window sample to a PNG named after it in `directory`, at Retina scale and at its window's
    /// size: the side panel's for the permission panel, the card's for the rest.
    ///
    /// - Parameter directory: Where to write the PNGs.
    /// - Returns: Whether every PNG was written.
    static func draw(to directory: String) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      return renders.map { render in
        PNG.write(
          OnboardingView(state: render.state, form: .constant(form(render.state))) { _ in }
            .environment(\.appIcon, AppIconImage.image),
          size: size(render.state),
          to: folder.appendingPathComponent("\(render.file).png")
        )
      }
      .allSatisfy { $0 }
    }

    /// The form a render draws: the one with a bad short name for that sample, else the form a new window opens with.
    ///
    /// - Parameter state: The sample drawn.
    /// - Returns: Its form.
    static func form(_ state: OnboardingState) -> OnboardingAccountForm {
      if state == PreviewStates.onboardingCreateBadName { return PreviewStates.onboardingFormBadName }
      return PreviewStates.onboardingForm
    }

    /// The window size a sample draws at.
    ///
    /// - Parameter state: The sample.
    /// - Returns: The side panel's size for the permission panel, else the card's.
    static func size(_ state: OnboardingState) -> CGSize {
      switch state.content {
      case .permissionPanel: OnboardingPlacement.size(guiding: true)
      case .loading, .screen: OnboardingPlacement.size(guiding: false)
      }
    }
  }
#endif
