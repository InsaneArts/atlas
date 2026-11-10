import Testing

@testable import AtlasUI

@Suite("Settings preview events")
@MainActor
struct SettingsPreviewEventsTests {
  @Test("the interactive preview opens the account clicked, not another, and drops its page on the way back")
  func theInteractivePreviewOpensTheAccountClicked() throws {
    let accounts = PreviewStates.settingsAccounts
    let staging = try #require(accounts.accounts.rows?.last)
    let opened = PreviewStates.settings(accounts, applying: .openAccount(staging.id))
    #expect(opened.account?.id == staging.id)
    #expect(opened.account?.title == "staging.acme.dev")
    #expect(opened.account?.level == .trusted)
    #expect(opened.account?.kindLine == "Website · added Nov 3")
    let back = PreviewStates.settings(opened, applying: .goBack)
    #expect(back.account == nil)
    let forward = PreviewStates.settings(back, applying: .goForward)
    #expect(forward.account?.id == staging.id, "Forward rebuilds the same account's page")
    #expect(PreviewStates.settings(accounts, applying: .openAccount("missing")).account == nil, "no stand-in page")
  }
}
