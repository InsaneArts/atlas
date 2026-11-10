import Testing

@testable import AtlasUI

@Suite("SettingsNavigation")
struct SettingsNavigationTests {
  @Test("going somewhere keeps the route left for Back, and Back and Forward walk the history both ways")
  func backAndForwardWalkTheHistory() {
    var navigation = SettingsNavigation(.page(.helper))
    navigation.go(to: .page(.general))
    navigation.go(to: .page(.accounts))
    #expect(navigation.back == [.page(.helper), .page(.general)])
    navigation.goBack()
    #expect(navigation.current == .page(.general))
    #expect(navigation.canGoForward)
    navigation.goBack()
    #expect(navigation.current == .page(.helper))
    #expect(!navigation.canGoBack)
    navigation.goForward()
    navigation.goForward()
    #expect(navigation.current == .page(.accounts))
    #expect(!navigation.canGoForward)
  }

  @Test("going to the route shown changes nothing")
  func theRouteShownIsANoOp() {
    var navigation = SettingsNavigation(.page(.about))
    navigation.go(to: .page(.about))
    #expect(navigation == SettingsNavigation(.page(.about)))
  }

  @Test("choosing a page after going back drops what Forward held, as a browser does")
  func aNewPageClearsForward() {
    var navigation = SettingsNavigation(.page(.helper))
    navigation.go(to: .page(.general))
    navigation.goBack()
    navigation.go(to: .page(.privacy))
    #expect(!navigation.canGoForward)
    #expect(navigation.back == [.page(.helper)])
  }

  @Test("Back and Forward with nowhere to go change nothing")
  func emptyStacksAreNoOps() {
    var navigation = SettingsNavigation(.page(.helper))
    navigation.goBack()
    navigation.goForward()
    #expect(navigation == SettingsNavigation(.page(.helper)))
  }

  @Test("an account removed while its page shows goes back to Accounts, without keeping it for Forward")
  func aRemovedAccountGoesBackToAccounts() {
    var navigation = SettingsNavigation(.page(.helper))
    navigation.go(to: .page(.accounts))
    navigation.go(to: .account("github"))
    navigation.keep(accounts: ["slack"])
    #expect(navigation.current == .page(.accounts))
    #expect(navigation.back == [.page(.helper)], "Accounts isn't kept twice in a row")
    #expect(!navigation.canGoForward)
  }

  @Test("routes to removed accounts leave both stacks, and listed ones stay")
  func removedAccountsLeaveTheHistory() {
    var navigation = SettingsNavigation(.page(.accounts))
    navigation.go(to: .account("github"))
    navigation.go(to: .page(.accounts))
    navigation.go(to: .account("slack"))
    navigation.go(to: .page(.about))
    navigation.goBack()
    navigation.goBack()
    #expect(navigation.current == .page(.accounts))
    navigation.keep(accounts: ["slack"])
    #expect(navigation.back == [], "github's page and the Accounts beside it fold away")
    #expect(navigation.forward == [.page(.about), .account("slack")])
  }

  @Test("an account's page sits under Accounts in the sidebar")
  func anAccountSitsUnderAccounts() {
    #expect(SettingsRoute.account("github").page == .accounts)
    #expect(SettingsRoute.page(.privacy).page == .privacy)
  }
}
