import Testing

@testable import AtlasUI

@Suite("AccountAction")
struct AccountActionTests {
  @Test("an opened site offers Sign Out, Forget, Reset and Remove with what each does, and an app offers Remove alone")
  func anOpenedAccountOffersItsActions() {
    let site = SettingsAccountRow(id: "s", letter: "G", title: "github.com", detail: "", isSite: true)
    #expect(AccountAction.of(site).map(\.button) == ["Sign Out…", "Forget…", "Reset…", "Remove…"])
    #expect(
      AccountAction.of(site).map(\.detail).prefix(3) == [
        "Opens the site's own sign-out, with agents paused",
        "Opens the browser's settings to delete this site's data, with agents paused",
        "Opens the browser's settings to clear its data, with agents paused",
      ]
    )
    #expect(AccountAction.of(site).map(\.event).first == .signOutAccount("s"))
    let app = SettingsAccountRow(id: "a", letter: "S", title: "Slack", detail: "")
    #expect(AccountAction.of(app).map(\.event) == [.askRemoveAccount("a")])
  }
}
