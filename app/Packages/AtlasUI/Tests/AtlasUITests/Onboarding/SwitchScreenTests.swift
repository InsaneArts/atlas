#if DEBUG
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("SwitchScreen")
  struct SwitchScreenTests {
    @Test("the switch screen keeps the Open Atlas there step when the opener is not installed")
    func keepsTheOpenAtlasStep() {
      var state = PreviewStates.onboardingSwitchScreen
      state.opensByItself = false
      let byHand = SwitchScreen.steps(state)[1]
      #expect(byHand.title == "Open Atlas there")
      #expect(byHand.detail == "In Applications. If macOS asks you to set anything up, choose Set Up Later")
      state.opensByItself = true
      let byItself = SwitchScreen.steps(state)[1]
      #expect(byItself.title == "Atlas opens there by itself")
      #expect(byItself.detail == "If macOS asks you to set anything up, choose Set Up Later")
    }
  }
#endif
