import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasService
import CoreGraphics
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings account icons")
@MainActor
struct SettingsAccountIconsTests {
  /// A 32-pixel picture.
  // swift-format-ignore: NeverForceUnwrap
  // A PNG made here always decodes.
  let picture = SiteIcons.largest(in: png(width: 32, height: 32))!

  @Test("an app shows its own icon and a site its icon, on its row and on its page, asked once each")
  func appsAndSitesShowTheirIcons() async throws {
    let status = try SettingsAccountsTests.reading([
      SettingsAccountsTests.account("site", "github.com", "github.com", added: SettingsAccountsTests.today),
      SettingsAccountsTests.account("app", "md.obsidian", "Obsidian", added: SettingsAccountsTests.today),
      SettingsAccountsTests.account("site", "acme.dev", "acme.dev", added: SettingsAccountsTests.today),
    ])
    let asked = AskedSites()
    let picture = picture
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), status: status) { dependencies in
      dependencies.appIcon = { $0 == "md.obsidian" ? picture : nil }
      dependencies.siteIcon = { site in
        await asked.note(site)
        return site == "github.com" ? picture : nil
      }
    }
    try await settle("github.com's icon never came") {
      SettingsAccountsTests.pane(controller).rows?.first?.icon != nil
    }
    let rows = try #require(SettingsAccountsTests.pane(controller).rows)
    #expect(rows.map(\.icon).map(Self.kind) == ["site", "app", "letter"])
    controller.send(.openAccount(rows[1].id))
    #expect(Self.kind(controller.state.account?.icon) == "app")
    controller.keepListedAccounts()
    #expect(await asked.sites.sorted() == ["acme.dev", "github.com"], "each site was asked once")
  }

  /// What an icon is, for comparing: an app's, a site's, or a letter.
  static func kind(_ icon: SettingsAccountIcon?) -> String {
    switch icon {
    case .app: "app"
    case .site: "site"
    case nil: "letter"
    }
  }
}
