import AsyncSupportTesting
import AtlasCommandsTesting
import AtlasService
import AtlasServiceTesting
import Foundation
import Observation
import Testing

@testable import AtlasApp
@testable import AtlasUI

@Suite("Settings account page", .serialized, AppKitState())
@MainActor
struct SettingsAccountPageTests {
  /// The status the controller reads, which a test changes as the list changes, and a window follows.
  @Observable
  final class Reading {
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

  @Test("while Atlas can't read its list the page keeps its name, its choices wait, and it recovers")
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
    controller.send(.select(.accounts))
    #expect(controller.state.route == .page(.accounts))
  }

  @Test("an account removed while its page shows goes back to Accounts")
  func aRemovedAccountGoesBackToAccounts() throws {
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
  }

  @Test("an account removed while Settings is closed reopens on Accounts, the list unreadable")
  func aRemovalWhileClosedOutlivesAnUnreadableList() async throws {
    let listed = try Self.listed()
    let reading = Reading(listed)
    let controller = Self.controller(reading)
    let ids = try #require(listed.overview.accounts?.map(\.id))
    let window = SettingsWindow(controller: controller) { _ in }
    controller.send(.select(.accounts))
    controller.send(.openAccount(ids[0]))
    window.close()
    reading.status = try SettingsAccountsTests.reading([
      SettingsAccountsTests.account("app", "com.tinyspeck.slackmacgap", "Slack", added: SettingsAccountsTests.today)
    ])
    for _ in 0..<20 where controller.session.route != .page(.accounts) {
      await afterQueuedWork()
    }
    #expect(controller.session.route == .page(.accounts), "the controller kept the removal with its window closed")
    reading.status = try status(["overview": ["mode": "observing"]])
    let reopened = SettingsWindow(controller: controller) { _ in }
    defer { reopened.close() }
    #expect(controller.state.route == .page(.accounts), "reopened on an unreadable list, the page stays gone")
  }

  @Test("an account renamed by a reading keeps its new name on its page when the list then can't be read")
  func aRenameOutlivesAnOutage() async throws {
    let listed = try Self.listed()
    let reading = Reading(listed)
    let controller = Self.controller(reading)
    let id = try #require(listed.overview.accounts?.first?.id)
    controller.send(.openAccount(id))
    reading.status = try SettingsAccountsTests.reading([
      SettingsAccountsTests.account("site", "github.com", "GitHub", added: SettingsAccountsTests.today)
    ])
    for _ in 0..<20 where controller.session.knownAccounts[id]?.name != "GitHub" {
      await afterQueuedWork()
    }
    reading.status = try status(["overview": ["mode": "observing"]])
    #expect(Self.page(controller)?.title == "GitHub")
    #expect(Self.page(controller)?.isUnreadable == true)
  }

  @Test("while an alert or the sign-in sheet is up, the sidebar and an account's page wait")
  func navigationWaitsUnderAModal() throws {
    let reading = Reading(try Self.listed())
    let controller = Self.controller(reading)
    controller.send(.select(.about))
    controller.send(.askRecover)
    try #require(controller.state.alert != nil)
    controller.send(.select(.privacy))
    #expect(controller.state.route == .page(.about), "under an alert")
    controller.send(.alertCancel)
    controller.send(.askSignIn(nil))
    try #require(controller.state.signIn != nil)
    controller.send(.openAccount("github"))
    #expect(controller.state.route == .page(.about), "under the sheet")
    controller.send(.cancelSignIn)
    controller.send(.select(.privacy))
    #expect(controller.state.route == .page(.privacy))
  }

  @Test("a level change that fails after the person left its page is still heard, and shows again on return")
  func aFailureAfterLeavingIsKept() async throws {
    let listed = try Self.listed()
    let reading = Reading(listed)
    let controller = Self.controller(reading) { dependencies in
      dependencies.links.setLevel = { _, _ in throw CocoaError(.fileWriteNoPermission) }
    }
    let id = try #require(listed.overview.accounts?.first?.id)
    controller.send(.select(.accounts))
    controller.send(.openAccount(id))
    controller.send(.select(.accounts))
    await controller.setLevel(.trusted, of: id)
    #expect(controller.state.account == nil)
    #expect(controller.state.accountNotice?.kind == .problem, "the result is kept for VoiceOver whatever shows")
    controller.send(.openAccount(id))
    #expect(controller.state.account?.notice == controller.state.accountNotice, "its page says it again")
  }

  @Test("a level shows at once while its change runs, the rest of the page as it is and only the picker waiting")
  func aLevelShowsAtOnce() async throws {
    let listed = try Self.listed()
    let reading = Reading(listed)
    let gate = IconLookupGate()
    let controller = Self.controller(reading) { dependencies in
      dependencies.links.setLevel = { _, _ in await gate.read() }
    }
    let id = try #require(listed.overview.accounts?.first?.id)
    controller.send(.openAccount(id))
    let changing = Task { await controller.setLevel(.trusted, of: id) }
    try await settle("the change never started") { gate.count == 1 }
    let page = try #require(Self.page(controller))
    #expect(page.level == .trusted && page.levelLine == SettingsWords.levelLine(.trusted))
    #expect(page.isEnabled, "Sign-In and Remove stay as they are")
    #expect(page.isChanging, "the picker waits")
    await controller.setLevel(.lookOnly, of: id)
    #expect(gate.count == 1, "a second change waits for the first")
    gate.release()
    await changing.value
    #expect(Self.page(controller)?.isChanging == false)
    #expect(Self.page(controller)?.level == .ask, "the reading after says what Atlas holds")
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
