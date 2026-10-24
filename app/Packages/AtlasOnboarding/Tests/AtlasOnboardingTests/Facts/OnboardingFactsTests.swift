import AtlasOnboardingTesting
import Foundation
import Testing

@testable import AtlasOnboarding

@Suite("OnboardingFacts")
struct OnboardingFactsTests {
  @Test("a reply that is not a status is refused")
  func refused() {
    #expect(OnboardingFacts(json: Data("not json".utf8)) == nil)
    #expect(OnboardingFacts(json: Data("[]".utf8)) == nil)
  }

  @Test("the deleted accounts' open logins are read by user ID, and none when the reply has none")
  func leftoverLogins() throws {
    #expect(try controller(leftovers: [502, 504]).leftoverLogins == [502, 504])
    #expect(try controller().leftoverLogins.isEmpty)
    let older = try facts(["role": "controller", "configured": false])
    #expect(older.leftoverLogins.isEmpty, "a reply without the field")
  }

  @Test("each side reads its own names: the person's account and candidates, or the account that asked")
  func names() throws {
    let person = try controller()
    #expect(person.candidates == [.init(name: "qa", realName: "QA Account")])
    #expect(person.myName == "Ismayil")
    let asked = try asker(installed: true, running: true, ax: true, screen: true)
    #expect(asked.asker.title == "Ismayil")
    #expect(asked.asker.uid == 501)
  }
}
