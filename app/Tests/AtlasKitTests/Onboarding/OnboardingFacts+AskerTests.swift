import Testing

@testable import AtlasKit

@Suite("OnboardingFacts+Asker")
struct OnboardingFactsAskerTests {
  @Test("unknown permissions keep what was known before, and known ones replace it")
  func keepingLastKnown() {
    var before = OnboardingFacts.Asker()
    before.accessibility = true
    before.screenCapture = true
    before.permissionsKnown = true
    let restarting = OnboardingFacts.Asker()
    #expect(restarting.keepingLastKnown(of: before, restarting: true).accessibility)
    #expect(restarting.keepingLastKnown(of: before, restarting: true).screenCapture)
    #expect(restarting.keepingLastKnown(of: before, restarting: true).permissionsKnown, "known from before")
    #expect(!restarting.keepingLastKnown(of: OnboardingFacts.Asker(), restarting: true).permissionsKnown)
    #expect(
      restarting.keepingLastKnown(of: OnboardingFacts.Asker(), restarting: false).permissionsKnown,
      "never read for too long: off, so they can be allowed"
    )
    var read = OnboardingFacts.Asker()
    read.permissionsKnown = true
    #expect(!read.keepingLastKnown(of: before, restarting: true).accessibility, "a reading that says off is off")
  }

  @Test("a job that ran still counts as running while it restarts, and as stopped once that took too long")
  func keepingRunning() {
    var before = OnboardingFacts.Asker()
    before.installed = true
    before.running = true
    var gone = OnboardingFacts.Asker()
    gone.installed = true
    #expect(gone.keepingLastKnown(of: before, restarting: true).running)
    #expect(!gone.keepingLastKnown(of: before, restarting: false).running, "too long: it stopped")
    gone.installed = false
    #expect(!gone.keepingLastKnown(of: before, restarting: true).running, "an uninstalled job does not run")
    var answered = OnboardingFacts.Asker()
    answered.installed = true
    answered.permissionsKnown = true
    #expect(!answered.keepingLastKnown(of: before, restarting: true).running, "a job that answered says itself")
  }
}
