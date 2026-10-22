import Testing

@testable import AtlasKit

@Suite("AccountNameRule")
struct AccountNameRuleTests {
  @Test(
    "a short name is refused here exactly where the command line would refuse it",
    arguments: [
      ("atlas", true), ("qa_helper-2", true), ("Atlas.Helper", true), ("has space", false), ("café", false),
      ("-dash", false), (".dot", false), (String(repeating: "x", count: 31), true),
      (String(repeating: "x", count: 32), false),
    ]
  )
  func rule(name: String, accepted: Bool) {
    #expect((AccountNameRule.problem(name) == nil) == accepted, "\(name)")
    #expect(
      PasswordRule.canCreate(fullName: "Atlas", accountName: name, password: "pw", verify: "pw") == accepted,
      "Create waits for a name the command line takes: \(name)"
    )
  }

  @Test(
    "the name a problem suggests is one the rule takes, made from what was typed",
    arguments: [
      ("Atlas Helper", "atlas-helper"), ("café bot", "cafe-bot"), ("-dash", "dash"), ("..", "atlas"),
      ("a  b!!c", "a-b-c"), (String(repeating: "x", count: 40), String(repeating: "x", count: 31)),
    ]
  )
  func suggestion(name: String, suggested: String) {
    #expect(AccountNameRule.suggestion(for: name) == suggested, "\(name)")
    #expect(AccountNameRule.problem(suggested) == nil, "\(suggested) is taken")
  }
}
