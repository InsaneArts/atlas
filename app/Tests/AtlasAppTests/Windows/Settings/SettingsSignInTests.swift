import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings, Sign In to a Site…")
struct SettingsSignInTests {
  @Test("the sheet opens empty or with a row's site, edits clear a refusal, and Continue waits for a site")
  @MainActor func theSheetFollowsItsField() throws {
    let controller = SettingsController.scripted(
      commands: ScriptedCommands([:]),
      status: try SettingsAccountsTests.reading([])
    )
    controller.send(.askSignIn(nil))
    #expect(controller.state.signIn == SignInSheetState())
    #expect(controller.state.signIn?.canContinue == false, "no site, no Continue")
    controller.send(.editSignInSite("github.com"))
    #expect(controller.state.signIn?.canContinue == true)
    controller.send(.cancelSignIn)
    #expect(controller.state.signIn == nil)
    controller.send(.askSignIn("gitlab.com"))
    #expect(controller.state.signIn?.site == "gitlab.com")
  }

  @Test("Continue starts the sign-in for the typed site and closes the sheet, or keeps it open saying why it couldn't")
  @MainActor func continueStartsOrSaysWhy() async throws {
    var asked: [String] = []
    var refusal: String? = "An agent's command is still running. Try again when it ends."
    let controller = SettingsController.scripted(
      commands: ScriptedCommands([:]),
      status: try SettingsAccountsTests.reading([])
    ) { dependencies in
      dependencies.links.startSignIn = { purpose, site, browser in
        asked.append("\(purpose.rawValue) \(site) \(browser ?? "-")")
        return refusal
      }
    }
    controller.send(.askSignIn(nil))
    controller.send(.editSignInSite(" github.com "))
    await controller.continueSignIn()
    #expect(asked == ["signIn github.com -"])
    #expect(controller.state.signIn?.refusal == refusal, "the refusal shows in the sheet")
    #expect(controller.state.signIn?.isWorking == false)
    refusal = nil
    await controller.continueSignIn()
    #expect(controller.state.signIn == nil, "started, the sheet closes")
  }

  @Test("Forget and Reset start for the account's site in its browser, a refusal said under the list")
  @MainActor func forgetAndResetStartForTheAccount() async throws {
    let site = SettingsAccountsTests.account("site", "github.com", "github.com", added: SettingsAccountsTests.today)
    var withBrowser = site
    withBrowser["browser"] = "org.mozilla.firefox"
    let status = try SettingsAccountsTests.reading([withBrowser])
    var asked: [String] = []
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), status: status) { dependencies in
      dependencies.links.startSignIn = { purpose, site, browser in
        asked.append("\(purpose.rawValue) \(site) \(browser ?? "-")")
        return purpose == .reset
          ? "An agent's background job is still running. Stop it, or try again when it ends." : nil
      }
    }
    let id = try #require(status.overview.accounts?.first?.id)
    controller.send(.openAccount(id))
    await controller.holdFor(.forget, account: id)
    await controller.holdFor(.reset, account: id)
    #expect(asked == ["forget github.com org.mozilla.firefox", "reset github.com org.mozilla.firefox"])
    #expect(controller.state.account?.notice?.kind == .problem, "the refusal shows on the account's page")
    #expect(SettingsAccountsTests.pane(controller).notice == nil)
  }

  @Test("clicking an account opens its own page, a site's with its sign-in, an app's without, and Back returns")
  @MainActor func anAccountOpensItsPage() throws {
    let controller = SettingsController.scripted(
      commands: ScriptedCommands([:]),
      status: try SettingsAccountsTests.reading([
        SettingsAccountsTests.account("site", "github.com", "github.com", added: SettingsAccountsTests.today),
        SettingsAccountsTests.account("app", "com.tinyspeck.slackmacgap", "Slack", added: SettingsAccountsTests.today),
      ])
    )
    controller.send(.select(.accounts))
    let rows = try #require(SettingsAccountsTests.pane(controller).rows)
    controller.send(.openAccount(rows[0].id))
    #expect(controller.state.route == .account(rows[0].id))
    #expect(controller.state.title == "github.com")
    #expect(controller.state.account?.isSite == true)
    controller.send(.goBack)
    controller.send(.openAccount(rows[1].id))
    #expect(controller.state.account?.title == "Slack")
    #expect(controller.state.account?.isSite == false, "only a site offers Sign Out, Forget and Reset")
    controller.send(.goBack)
    #expect(controller.state.route == .page(.accounts))
    #expect(controller.state.account == nil)
  }
}
