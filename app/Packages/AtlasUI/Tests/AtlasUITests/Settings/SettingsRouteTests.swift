import Testing

@testable import AtlasUI

@Suite("SettingsRoute")
struct SettingsRouteTests {
  @Test("the page of an account no longer listed goes back to Accounts")
  func aRemovedAccountGoesBackToAccounts() {
    #expect(SettingsRoute.account("github").kept(accounts: ["slack"]) == .page(.accounts))
  }

  @Test("a listed account's page and every other page stay")
  func listedAccountsAndPagesStay() {
    #expect(SettingsRoute.account("slack").kept(accounts: ["slack"]) == .account("slack"))
    #expect(SettingsRoute.page(.privacy).kept(accounts: []) == .page(.privacy))
  }

  @Test("an account's page sits under Accounts in the sidebar")
  func anAccountSitsUnderAccounts() {
    #expect(SettingsRoute.account("github").page == .accounts)
    #expect(SettingsRoute.page(.privacy).page == .privacy)
  }
}
