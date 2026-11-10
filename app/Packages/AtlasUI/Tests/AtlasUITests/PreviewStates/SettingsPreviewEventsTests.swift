import Testing

@testable import AtlasUI

@Suite("Settings preview events")
@MainActor
struct SettingsPreviewEventsTests {
  @Test("the interactive preview opens the account clicked, not another, and drops its page on leaving")
  func theInteractivePreviewOpensTheAccountClicked() throws {
    let accounts = PreviewStates.settingsAccounts
    let staging = try #require(accounts.accounts.rows?.last)
    let opened = PreviewStates.settings(accounts, applying: .openAccount(staging.id))
    #expect(opened.account?.id == staging.id)
    #expect(opened.account?.title == "staging.acme.dev")
    #expect(opened.account?.level == .trusted)
    #expect(opened.account?.kindLine == "Website · added Nov 3")
    #expect(PreviewStates.settings(opened, applying: .select(.accounts)).account == nil)
    #expect(PreviewStates.settings(accounts, applying: .openAccount("missing")).account == nil, "no stand-in page")
  }
}
