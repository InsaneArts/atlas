import Testing

@testable import AtlasKit

@Suite("PasswordRule")
struct PasswordRuleTests {
  @Test("passwords must match, fit 256 bytes and hold no control characters")
  func rule() {
    #expect(PasswordRule.problem(password: "one", verify: "two") == "The two passwords are different.")
    #expect(PasswordRule.problem(password: "bad\u{7}", verify: "two") == "Use a password without control characters.")
    let fits = String(repeating: "a", count: 256)
    #expect(PasswordRule.problem(password: fits, verify: fits) == nil)
    // 129 two-byte characters: 258 bytes, though fewer than 256 characters.
    let tooLong = String(repeating: "\u{E9}", count: 129)
    #expect(PasswordRule.problem(password: tooLong, verify: tooLong) == "That password is too long. Use a shorter one.")
    #expect(PasswordRule.problem(password: "one", verify: "") == nil, "nothing to say until both are typed")
    #expect(PasswordRule.canCreate(fullName: "Atlas", accountName: "atlas", password: "pw", verify: "pw"))
    #expect(!PasswordRule.canCreate(fullName: "Atlas", accountName: "atlas", password: "one", verify: "two"))
    #expect(!PasswordRule.canCreate(fullName: "Atlas", accountName: "atlas", password: fits + "a", verify: fits + "a"))
    #expect(!PasswordRule.canCreate(fullName: "", accountName: "atlas", password: "pw", verify: "pw"))
  }
}
