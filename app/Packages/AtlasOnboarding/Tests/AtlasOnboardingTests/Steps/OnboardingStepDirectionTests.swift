import Testing

@testable import AtlasOnboarding

@Suite("OnboardingStep+Direction")
struct OnboardingStepDirectionTests {
  @Test("going back slides the screens the other way")
  func backIsBack() {
    #expect(OnboardingStep.direction(from: .chooseHelper, to: .welcome) == .back)
    #expect(OnboardingStep.direction(from: .createAccount, to: .chooseHelper) == .back)
    #expect(OnboardingStep.direction(from: .switchToHelper, to: .chooseHelper) == .back, "Choose Another Account")
    #expect(OnboardingStep.direction(from: .done, to: .agents) == .back)
    #expect(OnboardingStep.direction(from: .welcome, to: .chooseHelper) == .forward)
    #expect(OnboardingStep.direction(from: .chooseHelper, to: .createAccount) == .forward)
    #expect(OnboardingStep.direction(from: .agents, to: .done) == .forward)
    #expect(OnboardingStep.direction(from: .permissions, to: .helperDone) == .forward)
  }

  @Test("a blocked screen, and the same screen read again, move forward")
  func blockedMovesOn() {
    #expect(OnboardingStep.direction(from: .done, to: .blocked("x")) == .forward)
    #expect(OnboardingStep.direction(from: .blocked("x"), to: .welcome) == .forward)
    #expect(OnboardingStep.direction(from: .switchToHelper, to: .switchToHelper) == .forward)
  }
}
