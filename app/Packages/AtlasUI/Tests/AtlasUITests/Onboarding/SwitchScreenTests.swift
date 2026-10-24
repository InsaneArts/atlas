#if DEBUG
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("SwitchScreen")
  struct SwitchScreenTests {
    @Test("the switch screen says there is nothing to set up when Atlas opens by itself, and how to open it when not")
    func keepsTheOpenAtlasStep() {
      var state = PreviewStates.onboardingSwitchScreen
      state.opensByItself = false
      let byHand = SwitchScreen.steps(state)[1]
      #expect(byHand.title == "Open Atlas there")
      #expect(byHand.detail == "In Applications. If macOS asks you to set anything up, choose Set Up Later")
      state.opensByItself = true
      let byItself = SwitchScreen.steps(state)[1]
      #expect(byItself.title == "Atlas opens there by itself")
      #expect(byItself.detail == "Nothing to set up first", "macOS's welcome questions were answered already")
    }

    @Test("what happens next takes two lines at most, while a failure under the steps wraps as far as it needs")
    func onlyTheWaitingLineIsLimited() {
      var state = PreviewStates.onboardingSwitchScreen
      state.waiting = "Waiting for a helper account with a very long name to log in…"
      #expect(SwitchScreen.statusLines(state) == OnboardingLayout.nameLines)
      state.waiting = nil
      #expect(SwitchScreen.statusLines(state) == nil, "a failure's cause and what to do are never cut")
    }
  }
#endif
