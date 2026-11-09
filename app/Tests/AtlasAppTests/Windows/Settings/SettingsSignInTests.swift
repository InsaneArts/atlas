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
      dependencies.links.startSignIn = { kind, site, browser in
        asked.append("\(kind.rawValue) \(site) \(browser ?? "-")")
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
      dependencies.links.startSignIn = { kind, site, browser in
        asked.append("\(kind.rawValue) \(site) \(browser ?? "-")")
        return kind == .reset ? "An agent's background job is still running. Stop it, or try again when it ends." : nil
      }
    }
    let id = try #require(status.overview.accounts?.first?.id)
    await controller.holdFor(.forget, account: id)
    await controller.holdFor(.reset, account: id)
    #expect(asked == ["forget github.com org.mozilla.firefox", "reset github.com org.mozilla.firefox"])
    #expect(SettingsAccountsTests.pane(controller).notice?.kind == .problem)
  }

  @Test("the sheet says what Atlas does while the person signs in")
  func theSheetsWords() {
    #expect(
      SignInSheet.note
        == "Atlas pauses agents and hides the helper's screen from them while you sign in. "
        + "Paste from your password manager works."
    )
  }
}
