import SwiftUI

/// Log in to the helper once, to set Atlas up there: three steps joined by a rail, the one to do now lit (v3 D6, S3),
/// and one status line under them. Lock and Switch is the footer's.
struct SwitchScreen: View {
  /// The steps and what happens next.
  let state: OnboardingSwitchScreenState
  /// The status under the steps while a command runs or failed.
  let note: WorkNote

  /// The three steps, then what happens next or what runs.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(label: "Helper account", title: state.title, tail: OnboardingScreen.switchTail)
      StepList(steps: Self.steps(state))
        .padding(.top, OnboardingLayout.Rhythm.paragraph)
        .pagePiece(.content)
    } bottom: {
      if let status {
        status
          .lineLimit(Self.statusLines(state))
          .truncationMode(.middle)
          .padding(.top, OnboardingLayout.Rhythm.content)
          .pagePiece(.status)
      }
    }
  }

  /// What happens next while nothing runs or failed, else the work note.
  private var status: StatusLine? {
    guard let waiting = state.waiting else { return StatusLine(note: note) }
    return StatusLine(kind: .working, text: waiting)
  }

  /// How many lines the status line may take: two at most for what happens next, which names the helper and is cut in
  /// its middle; as many as a work note needs, so a failure never loses its cause or what to do.
  ///
  /// - Parameter state: The switch screen.
  /// - Returns: The most lines, or nil for no limit.
  static func statusLines(_ state: OnboardingSwitchScreenState) -> Int? {
    state.waiting == nil ? nil : OnboardingLayout.nameLines
  }

  /// Lock and switch, Atlas opening there, and allowing it, each where the helper's progress puts it.
  ///
  /// - Parameter state: The switch screen.
  /// - Returns: The three steps.
  static func steps(_ state: OnboardingSwitchScreenState) -> [StepList.Step] {
    let opening = openStep(opensByItself: state.opensByItself)
    return [
      StepList.Step(title: "Lock and switch", detail: state.logInDetail, state: state.progress.logIn),
      StepList.Step(title: opening.title, detail: opening.detail, state: state.progress.openAtlas),
      StepList.Step(
        title: "Allow Atlas twice, then switch back",
        detail: "Atlas there shows you how",
        state: state.progress.allow
      ),
    ]
  }

  /// The second step's words: Atlas opens in the helper by itself once the helper was readied; else the person opens
  /// it from Applications.
  ///
  /// - Parameter opensByItself: Whether Atlas opens in the helper by itself.
  /// - Returns: The step's title and detail.
  static func openStep(opensByItself: Bool) -> (title: String, detail: String) {
    if opensByItself {
      return ("Atlas opens there by itself", "If macOS asks you to set anything up, choose Set Up Later")
    }
    return ("Open Atlas there", "In Applications. If macOS asks you to set anything up, choose Set Up Later")
  }
}

#if DEBUG
  #Preview("Switch") {
    SwitchScreen(state: PreviewStates.onboardingSwitchScreen, note: .none)
      .onboardingSample()
  }

  #Preview("Atlas opened by hand") {
    var state = PreviewStates.onboardingSwitchScreen
    state.opensByItself = false
    return SwitchScreen(state: state, note: .none)
      .onboardingSample()
  }
#endif
