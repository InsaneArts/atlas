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

  @Test("the level picker reads as the system's picker, named for what agents can do, its value the level chosen")
  func theLevelPickerSpeaks() {
    #expect(AccountLevelPicker.label == "What agents can do here")
    #expect(AccountLevelPicker.spokenValue(.ask) == "Ask Before Sending")
    #expect(AccountLevelPicker.spokenValue(.trusted) == "Trusted")
  }

  @Test("arrows and VoiceOver's adjustment step one level at a time and stop at either end")
  func theLevelPickerSteps() {
    #expect(AccountLevelPicker.stepped(.lookOnly, by: 1) == .ask)
    #expect(AccountLevelPicker.stepped(.ask, by: 1) == .trusted)
    #expect(AccountLevelPicker.stepped(.trusted, by: 1) == .trusted)
    #expect(AccountLevelPicker.stepped(.ask, by: -1) == .lookOnly)
    #expect(AccountLevelPicker.stepped(.lookOnly, by: -1) == .lookOnly)
  }

  @Test("while a level change runs the picker keeps its focus and takes no new pick, then takes them again")
  func theLevelPickerWaitsWithoutLosingFocus() {
    #expect(!AccountLevelPicker.takes(.trusted, level: .ask, isBusy: true), "a pick waits while a change runs")
    #expect(AccountLevelPicker.takes(.trusted, level: .ask, isBusy: false))
    #expect(!AccountLevelPicker.takes(.ask, level: .ask, isBusy: false), "the level chosen sends nothing")
  }
}
