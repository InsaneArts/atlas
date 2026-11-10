import Testing

@testable import AtlasUI

@Suite("AccountAction")
struct AccountActionTests {
  @Test("a site's page offers Sign Out, Forget and Reset with what each does, and every page offers Remove")
  func anAccountsPageOffersItsActions() {
    let signIn = AccountAction.signIn(of: "s")
    #expect(signIn.map(\.button) == ["Sign Out…", "Forget…", "Reset…"])
    #expect(
      signIn.map(\.detail) == [
        "Opens the site's own sign-out, with agents paused",
        "Opens the browser's settings to delete this site's data, with agents paused",
        "Opens the browser's settings to clear its data, with agents paused",
      ]
    )
    #expect(signIn.map(\.event) == [.signOutAccount("s"), .forgetAccount("s"), .resetBrowser("s")])
    #expect(AccountAction.remove("a").event == .askRemoveAccount("a"))
    #expect(AccountAction.remove("a").button == "Remove…")
  }

  @Test("each level's tile carries its own symbol")
  func eachLevelHasItsSymbol() {
    #expect(Set(AccountLevelChoice.allCases.map(\.symbol)).count == AccountLevelChoice.allCases.count)
  }

  @Test("an account's page says when Atlas can't read its list, its level unknown")
  @MainActor
  func anUnreadablePageSaysSo() {
    #expect(PreviewStates.settingsAccountUnreadablePage.isUnreadable)
    #expect(!PreviewStates.settingsAccountUnreadablePage.isEnabled)
    #expect(!PreviewStates.settingsAccountSitePage.isUnreadable)
  }
}
