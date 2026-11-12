import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings accounts")
struct SettingsAccountsTests {
  /// Nov 8 2026, 12:00 UTC, in milliseconds since 1970.
  static let today: Double = 1_794_139_200_000
  /// One day, in milliseconds.
  static let day: Double = 86_400_000

  /// A status listing `accounts`, with `note` the newest added after a password.
  static func reading(_ accounts: [[String: Any]], note: [String: Any]? = nil) throws -> ViewerStatus {
    var overview: [String: Any] = ["mode": "observing", "accounts": accounts]
    overview["accountNote"] = note
    return try status(["overview": overview])
  }

  /// An account as the service lists it.
  static func account(_ kind: String, _ id: String, _ name: String, added: Double, how: String = "byHand") -> [String:
    Any]
  {
    ["kind": kind, "id": id, "name": name, "added": added, "how": how]
  }

  /// The snapshot `controller` draws, told on a fixed day in UTC and in US English.
  @MainActor static func pane(_ controller: SettingsController) -> SettingsAccountsState {
    var snapshot = controller.snapshot
    snapshot.now = Date(timeIntervalSince1970: today / 1000)
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "UTC") ?? .gmt
    calendar.locale = Locale(identifier: "en_US")
    snapshot.calendar = calendar
    return SettingsPresenter.accounts(snapshot)
  }

  @Test("the page sits between Activity and Privacy, after the sidebar's gap")
  func thePageSitsBetweenActivityAndPrivacy() {
    #expect(SettingsPage.allCases == [.helper, .general, .activity, .accounts, .privacy, .integrations, .about])
    #expect(SettingsWords.page(.accounts) == "Accounts")
    #expect(SettingsPage.accounts.group != SettingsPage.activity.group)
  }

  @Test("each account says its level and when it was added, an app saying it is one")
  @MainActor func eachAccountSaysWhenItWasAdded() throws {
    let status = try Self.reading([
      Self.account("site", "github.com", "github.com", added: Self.today),
      Self.account("app", "com.tinyspeck.slackmacgap", "Slack", added: Self.today - Self.day),
      Self.account("site", "acme.dev", "acme.dev", added: Self.today - 5 * Self.day),
    ])
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), status: status)
    let rows = try #require(Self.pane(controller).rows)
    #expect(rows.map(\.title) == ["github.com", "Slack", "acme.dev"])
    #expect(
      rows.map(\.detail) == [
        "Ask before sending · added today", "Ask before sending · App · added yesterday",
        "Ask before sending · added Nov 3",
      ]
    )
    #expect(rows.map(\.letter) == ["G", "S", "A"])
  }

  @Test("with none, the pane says so; when Atlas can't read its list, it never says none")
  @MainActor func noneAndUnreadAreApart() throws {
    let none = SettingsController.scripted(commands: ScriptedCommands([:]), status: try Self.reading([]))
    #expect(Self.pane(none).rows == [])
    #expect(Self.pane(none).empty.headline == "No accounts yet")
    let unread = SettingsController.scripted(commands: ScriptedCommands([:]), status: try status(["overview": [:]]))
    #expect(Self.pane(unread).rows == nil)
  }

  @Test("the panel's line names one, two, or the first two and how many more")
  func thePanelLineNamesTheAccounts() {
    #expect(PanelWords.accountsLine([]) == nil)
    #expect(PanelWords.accountsLine(["github.com"]) == "Accounts: github.com")
    #expect(PanelWords.accountsLine(["github.com", "Slack"]) == "Accounts: github.com and Slack")
    #expect(PanelWords.accountsLine(["github.com", "Slack", "acme.dev"]) == "Accounts: github.com, Slack and 1 more")
    #expect(PanelWords.accountsLine(["a", "b", "c", "d"]) == "Accounts: a, b and 2 more")
  }

  @Test("the note that Atlas added an account shows once, and stays read across launches")
  @MainActor func theNoteShowsOnce() throws {
    let added = Self.account("site", "github.com", "github.com", added: Self.today, how: "takeControl")
    let status = try Self.reading([added], note: ["name": "github.com", "added": Self.today])
    var saved: Double = 0
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), status: status) { dependencies in
      dependencies.preferences.loadAccountNoteRead = { saved }
      dependencies.preferences.saveAccountNoteRead = { saved = $0 }
    }
    #expect(
      Self.pane(controller).note
        == "Added github.com after you typed a password there. Agents ask you before sending there."
    )
    #expect(Self.pane(controller).noteAccount == "site\u{1F}github.com\u{1F}", "Change… sets that account's level")

    controller.send(.dismissAccountNote)
    #expect(Self.pane(controller).note == nil)
    #expect(saved == Self.today)

    let relaunched = SettingsController.scripted(commands: ScriptedCommands([:]), status: status) { dependencies in
      dependencies.preferences.loadAccountNoteRead = { saved }
    }
    #expect(Self.pane(relaunched).note == nil)
  }

  @Test("an app the person chooses is added, and an account is removed only once they confirm")
  @MainActor func addAndRemove() async throws {
    let status = try Self.reading([Self.account("site", "github.com", "github.com", added: Self.today)])
    var asked: [String] = []
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), status: status) { dependencies in
      dependencies.chooseApp = { ChosenApp(bundleID: "com.tinyspeck.slackmacgap", name: "Slack") }
      dependencies.links.addAccount = { app, name in asked.append("add \(app) \(name)") }
      dependencies.links.removeAccount = { account in asked.append("remove \(account.key)") }
    }

    await controller.addApp()
    #expect(asked == ["add com.tinyspeck.slackmacgap Slack"])

    let id = try #require(status.overview.accounts?.first?.id)
    controller.send(.askRemoveAccount(id))
    #expect(controller.state.alert?.title == "Remove github.com from Accounts?")
    #expect(
      controller.state.alert?.message
        == "Atlas stops listing it. Removing it from the list doesn't sign the helper out."
    )
    #expect(asked.count == 1, "nothing is removed before the person confirms")
    await controller.confirmAlert()
    #expect(asked.last == "remove github.com")
    #expect(controller.state.alert == nil)
  }

  @Test("each row shows its level, and picking one sends that level for that account")
  @MainActor func theLevelControlSendsTheLevel() async throws {
    var trusted = Self.account("site", "staging.acme.dev", "staging.acme.dev", added: Self.today)
    trusted["level"] = "trusted"
    let status = try Self.reading([
      Self.account("app", "com.tinyspeck.slackmacgap", "Slack", added: Self.today), trusted,
    ])
    var sent: [String] = []
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), status: status) { dependencies in
      dependencies.links.setLevel = { level, account in sent.append("\(account.key) \(level.rawValue)") }
    }
    #expect(Self.pane(controller).rows?.map(\.level) == [.ask, .trusted], "Ask Before Sending unless chosen")
    let slack = try #require(status.overview.accounts?.first?.id)
    await controller.setLevel(.lookOnly, of: slack)
    #expect(sent == ["com.tinyspeck.slackmacgap lookOnly"])
    await controller.setLevel(.ask, of: slack)
    #expect(sent.count == 1, "the level it has already is not sent again")
  }

  @Test("while General keeps no pictures of agent actions, the pane says so under the list, and not while it does")
  @MainActor func picturesOffIsSaid() throws {
    let status = try Self.reading([Self.account("site", "github.com", "github.com", added: Self.today)])
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), status: status)
    var snapshot = controller.snapshot
    #expect(SettingsPresenter.accounts(snapshot).picturesOff == nil, "pictures are kept by default")
    snapshot.pictures.shown.isOn = false
    #expect(
      SettingsPresenter.accounts(snapshot).picturesOff == "Activity won't keep pictures of what agents do here."
    )
  }

  @Test("a change that failed says why under the list")
  @MainActor func aFailedChangeSaysWhy() async throws {
    let controller = SettingsController.scripted(commands: ScriptedCommands([:]), status: try Self.reading([])) {
      dependencies in
      dependencies.chooseApp = { ChosenApp(bundleID: "com.example.app", name: "Example") }
      dependencies.links.addAccount = { _, _ in throw ServiceError("The accounts list is full; remove one first") }
    }
    await controller.addApp()
    let notice = Self.pane(controller).notice
    #expect(notice?.kind == .problem)
    #expect(notice?.text.hasPrefix("Atlas couldn't change its accounts list.") == true)
    #expect(Self.pane(controller).isWorking == false)
  }
}
