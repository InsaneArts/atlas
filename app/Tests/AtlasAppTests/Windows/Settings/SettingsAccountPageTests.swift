import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings account page")
@MainActor
struct SettingsAccountPageTests {
  /// The status the controller reads, which a test changes as the list changes.
  private final class Reading {
    /// The status now.
    var status: ViewerStatus

    /// Creates the reading.
    ///
    /// - Parameter status: The status at first.
    init(_ status: ViewerStatus) {
      self.status = status
    }
  }

  /// github.com and Slack, listed today.
  private static func listed() throws -> ViewerStatus {
    var slack = SettingsAccountsTests.account(
      "app",
      "com.tinyspeck.slackmacgap",
      "Slack",
      added: SettingsAccountsTests.today - SettingsAccountsTests.day
    )
    slack["level"] = "trusted"
    return try SettingsAccountsTests.reading([
      SettingsAccountsTests.account("site", "github.com", "github.com", added: SettingsAccountsTests.today), slack,
    ])
  }

  /// A controller over `reading`, told on the tests' fixed day.
  ///
  /// - Parameters:
  ///   - reading: The status it reads.
  ///   - edit: Changes its dependencies further.
  /// - Returns: The controller.
  private static func controller(
    _ reading: Reading,
    edit: @escaping (inout SettingsDependencies) -> Void = { _ in }
  ) -> SettingsController {
    SettingsController.scripted(
      commands: ScriptedCommands([:]),
      edit: { dependencies in
        dependencies.links.status = { (reading.status, false) }
        edit(&dependencies)
      }
    )
  }

  /// The page `controller` shows, told on the tests' fixed day in UTC.
  ///
  /// - Parameter controller: The controller.
  /// - Returns: The page, if one shows.
  private static func page(_ controller: SettingsController) -> SettingsAccountPageState? {
    var snapshot = controller.snapshot
    snapshot.now = Date(timeIntervalSince1970: SettingsAccountsTests.today / 1000)
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "UTC") ?? .gmt
    snapshot.calendar = calendar
    return SettingsPresenter.accountPage(snapshot)
  }

  @Test("a site's page says it is a website and when it was added, an app's what it is, each with its level's words")
  func aSiteAndAnAppReadApart() throws {
    let reading = Reading(try Self.listed())
    let controller = Self.controller(reading)
    let ids = try #require(reading.status.overview.accounts?.map(\.id))
    controller.send(.openAccount(ids[0]))
    let site = try #require(Self.page(controller))
    #expect(site.kindLine == "Website · added today")
    #expect(site.level == .ask)
    #expect(site.levelLine == "Agents ask you before sending there.")
    #expect(site.isSite && site.isEnabled)
    controller.send(.openAccount(ids[1]))
    let app = try #require(Self.page(controller))
    #expect(app.kindLine == "App · added yesterday")
    #expect(app.levelLine == "Agents act there without asking.")
    #expect(!app.isSite)
  }

  @Test("while Atlas can't read its list the page keeps its name, its choices wait, Back still works, and it recovers")
  func anUnreadableListKeepsThePage() throws {
    let listed = try Self.listed()
    let reading = Reading(listed)
    let controller = Self.controller(reading)
    let id = try #require(listed.overview.accounts?.first?.id)
    controller.send(.select(.accounts))
    controller.send(.openAccount(id))
    reading.status = try status(["overview": ["mode": "observing"]])
    let unreadable = try #require(Self.page(controller))
    #expect(unreadable.title == "github.com")
    #expect(unreadable.isUnreadable && !unreadable.isEnabled)
    #expect(controller.state.title == "github.com")
    reading.status = listed
    #expect(Self.page(controller)?.isEnabled == true, "the page comes back once the list reads again")
    reading.status = try status(["overview": ["mode": "observing"]])
    controller.send(.goBack)
    #expect(controller.state.route == .page(.accounts))
  }

  @Test("an account removed while its page shows goes back to Accounts, and leaves Back's and Forward's history")
  func aRemovedAccountLeavesTheHistory() throws {
    let listed = try Self.listed()
    let reading = Reading(listed)
    let controller = Self.controller(reading)
    let ids = try #require(listed.overview.accounts?.map(\.id))
    controller.send(.select(.accounts))
    controller.send(.openAccount(ids[0]))
    reading.status = try SettingsAccountsTests.reading([
      SettingsAccountsTests.account(
        "app",
        "com.tinyspeck.slackmacgap",
        "Slack",
        added: SettingsAccountsTests.today
      )
    ])
    #expect(controller.state.route == .page(.accounts))
    #expect(controller.state.account == nil)
    #expect(!controller.state.navigation.canGoForward)
    controller.send(.goBack)
    #expect(controller.state.route == .page(.helper), "Back skips the removed account's page")
  }

  @Test("a level change that failed says why on the account's page, not on Accounts")
  func aFailedLevelChangeShowsOnItsPage() async throws {
    let listed = try Self.listed()
    let reading = Reading(listed)
    let controller = Self.controller(reading) { dependencies in
      dependencies.links.setLevel = { _, _ in throw CocoaError(.fileWriteNoPermission) }
    }
    let id = try #require(listed.overview.accounts?.first?.id)
    controller.send(.openAccount(id))
    await controller.setLevel(.trusted, of: id)
    #expect(controller.state.account?.notice?.kind == .problem)
    #expect(controller.state.accounts.notice == nil)
  }
}
