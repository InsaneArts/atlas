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
      state.showsManualOpening = false
      let byItself = SwitchScreen.steps(state)[1]
      #expect(byItself.title == "Atlas opens there by itself")
      #expect(byItself.detail == "Nothing to set up first", "macOS's welcome questions were answered already")
    }

    @Test("an unanswered helper keeps the manual opening instructions alongside Atlas opening by itself")
    func anUnansweredHelperKeepsTheManualOpening() {
      let state = PreviewStates.onboardingSwitchScreen
      let opening = SwitchScreen.steps(state)[1]
      #expect(opening.title == "Atlas opens there by itself")
      #expect(opening.detail == "Nothing to set up first. If needed, open Atlas there, in Applications")
    }

    @Test("a slow login or problem replaces ordinary waiting and wraps without cutting its reassurance")
    func workNotesTakePrecedence() throws {
      var state = PreviewStates.onboardingSwitchScreen
      state.waiting = "Waiting for Atlas…"
      for note in [WorkNote.working("Still opening. This can take a few minutes."), .problem("Try again.")] {
        let screen = SwitchScreen(state: state, note: note)
        let expected = try #require(StatusLine(note: note))
        #expect(screen.status?.text == expected.text)
        #expect(screen.status?.kind == expected.kind)
        #expect(SwitchScreen.statusLines(state, note: note) == nil)
      }
      #expect(SwitchScreen(state: state, note: .none).status?.text == state.waiting)
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
