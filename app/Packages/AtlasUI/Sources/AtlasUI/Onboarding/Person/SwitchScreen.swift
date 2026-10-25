import SwiftUI

/// Log in to the helper once, to set Atlas up there: three steps joined by a rail, the one to do now lit, and one
/// status line under them, which says nothing until Lock and Switch has been pressed. Lock and Switch is the footer's.
struct SwitchScreen: View {
  /// The steps and what happens next.
  let state: OnboardingSwitchScreenState
  /// The status under the steps while a command runs or failed.
  let note: WorkNote

  /// The three steps, then what happens next or what runs, in the line's kept place.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(label: "Helper account", title: state.title, tail: OnboardingScreen.switchTail)
      StepList(steps: Self.steps(state))
        .padding(.top, OnboardingLayout.Rhythm.paragraph)
      StatusSlot(line: status)
        .lineLimit(Self.statusLines(state, note: note))
        .truncationMode(.middle)
    }
  }

  /// What happens next while nothing runs or failed, else the work note.
  var status: StatusLine? {
    if let line = StatusLine(note: note) { return line }
    guard let waiting = state.waiting else { return nil }
    return StatusLine(kind: .working, text: waiting)
  }

  /// How many lines the status line may take: two at most for what happens next, which names the helper and is cut in
  /// its middle; as many as a work note needs, so a failure never loses its cause or what to do.
  ///
  /// - Parameters:
  ///   - state: The switch screen.
  ///   - note: Work or a problem, which takes as many lines as it needs.
  /// - Returns: The most lines, or nil for no limit.
  static func statusLines(_ state: OnboardingSwitchScreenState, note: WorkNote = .none) -> Int? {
    state.waiting == nil || StatusLine(note: note) != nil ? nil : OnboardingLayout.nameLines
  }

  /// Logging in, Atlas opening there, and allowing it, each where the helper's progress puts it.
  ///
  /// - Parameter state: The switch screen.
  /// - Returns: The three steps.
  static func steps(_ state: OnboardingSwitchScreenState) -> [StepList.Step] {
    let opening = openStep(opensByItself: state.opensByItself, showsManualOpening: state.showsManualOpening)
    return [
      StepList.Step(title: state.logInTitle, detail: "Pick it on the lock screen", state: state.progress.logIn),
      StepList.Step(title: opening.title, detail: opening.detail, state: state.progress.openAtlas),
      StepList.Step(
        title: "Allow Atlas twice, then switch back",
        detail: "Atlas there shows you how",
        state: state.progress.allow
      ),
    ]
  }

  /// The second step's words: Atlas opens in the helper by itself once the helper was readied, which also skips
  /// macOS's welcome questions there, so there is nothing to set up; else the person opens it from Applications.
  /// Until Atlas answers, the manual opening instructions stay alongside the automatic opening.
  ///
  /// - Parameters:
  ///   - opensByItself: Whether Atlas opens in the helper by itself.
  ///   - showsManualOpening: Whether to include how to open Atlas from Applications.
  /// - Returns: The step's title and detail.
  static func openStep(opensByItself: Bool, showsManualOpening: Bool) -> (title: String, detail: String) {
    if opensByItself {
      let detail =
        showsManualOpening
        ? "Nothing to set up first. If needed, open Atlas there, in Applications"
        : "Nothing to set up first"
      return ("Atlas opens there by itself", detail)
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
