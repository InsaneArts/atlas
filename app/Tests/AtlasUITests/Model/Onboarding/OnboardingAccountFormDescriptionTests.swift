import Testing

@testable import AtlasUI

@Suite("OnboardingAccountForm+Description")
struct OnboardingAccountFormDescriptionTests {
  @Test("a printed account form never shows its password")
  func neverPrintsThePassword() {
    let form = OnboardingAccountForm(
      fullName: "Test Helper",
      accountName: "testhelper",
      password: "hunter2",
      verify: "hunter2"
    )
    let optional: OnboardingAccountForm? = form
    var dumped = ""
    dump(form, to: &dumped)
    var dumpedInArray = ""
    dump([form], to: &dumpedInArray)
    var dumpedOptional = ""
    dump(optional, to: &dumpedOptional)
    let renderings = [
      String(describing: form),
      String(reflecting: form),
      dumped,
      String(describing: [form]),
      String(reflecting: [form]),
      dumpedInArray,
      String(describing: optional),
      String(reflecting: optional),
      dumpedOptional,
    ]
    for text in renderings {
      #expect(!text.contains("hunter2"), "\(text)")
      #expect(text.contains("testhelper") && text.contains("Test Helper"), "what is not secret still prints")
    }
  }
}
