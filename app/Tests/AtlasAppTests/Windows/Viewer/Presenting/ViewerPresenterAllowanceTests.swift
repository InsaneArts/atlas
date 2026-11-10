import AtlasService
import AtlasServiceTesting
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("ViewerPresenter, what let an agent act in a listed account")
@MainActor
struct ViewerPresenterAllowanceTests {
  /// github.com in Chrome, as a row names it.
  static let github = ActivityLog.Account(
    name: "github.com",
    key: AccountKey(kind: .site, id: "github.com", browser: "com.google.Chrome")
  )

  /// One of Claude Code's clicks in Chrome, `age` seconds before the reading.
  ///
  /// - Parameters:
  ///   - id: The row's id.
  ///   - says: What it did.
  ///   - label: The control it named.
  ///   - age: How long ago, in seconds.
  ///   - account: The listed account it acted in.
  ///   - allowed: What let it act there.
  /// - Returns: The row.
  static func click(
    _ id: String,
    _ says: String,
    label: String,
    age: Double,
    account: ActivityLog.Account? = github,
    allowed: ActivityLog.Allowance? = nil
  ) -> ActivityLog.Entry {
    ActivityLog.Entry(
      id: id,
      says: says,
      ageSeconds: age,
      session: ViewerActivityFixtures.claude,
      verb: "click",
      app: "Google Chrome",
      label: label,
      bundleId: "com.google.Chrome",
      account: account,
      allowed: allowed
    )
  }

  @Test(
    "each step says what let it act in its account, the person's own allowing in the accent and Trusted quiet",
    arguments: [
      (ActivityLog.Allowance.task, AllowanceBadge(words: "You allowed for this task", tone: .accent)),
      (.once, AllowanceBadge(words: "You allowed once", tone: .accent)),
      (.trusted, AllowanceBadge(words: "Trusted", tone: .quiet)),
    ]
  )
  func eachAllowanceHasItsBadge(allowed: ActivityLog.Allowance, badge: AllowanceBadge) throws {
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.click("c1", "clicked Merge on github.com", label: "Merge", age: 3, allowed: allowed)
    ])
    let activity = ViewerActivityFixtures.opened(snapshot)
    let step = try #require(ViewerActivityFixtures.steps(activity).first)
    #expect(step.allowance == badge)
    #expect(step.account == nil, "its run names the account, so the step does not again")
    #expect(step.label.contains(badge.words), "VoiceOver reads it too")
    let run = try #require(ViewerActivityFixtures.runs(activity).first)
    #expect(run.allowance == badge, "a run whose steps agree says it too")
    #expect(run.account == "github.com")
    #expect(run.name == "github.com", "a browser's run is titled by the site")
    #expect(step.words == ViewerStepWords("Clicked", target: "Merge"), "its step leaves the site to the run")
  }

  @Test("a step in no listed account, or one that needed no allowing, has no badge, and its run keeps the app's name")
  func noAccountNoBadge() throws {
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.click("c2", "clicked Pull requests on github.com", label: "Pull requests", age: 3),
      Self.click("c1", "clicked Search in Google Chrome", label: "Search", age: 5, account: nil),
    ])
    let activity = ViewerActivityFixtures.opened(snapshot)
    let steps = ViewerActivityFixtures.steps(activity)
    #expect(steps.count == 2)
    #expect(steps.allSatisfy { $0.allowance == nil })
    #expect(steps.map(\.account) == [nil, "github.com"], "oldest first, the step in the account naming it")
    let run = try #require(ViewerActivityFixtures.runs(activity).first)
    #expect(run.allowance == nil)
    #expect(run.name == "Google Chrome", "a run partly outside the account keeps the browser's name")
    #expect(steps.last?.words == ViewerStepWords("Clicked", target: "Pull requests"))
    #expect(steps.last?.label.contains("github.com") == true, "VoiceOver reads the account too")
  }

  @Test("a run whose steps were allowed differently names no one allowance, and each step keeps its own")
  func aMixedRunHasNoBadge() throws {
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.click("c2", "clicked Merge on github.com", label: "Merge", age: 3, allowed: .once),
      Self.click("c1", "clicked Close on github.com", label: "Close", age: 5, allowed: .task),
    ])
    let activity = ViewerActivityFixtures.opened(snapshot)
    let run = try #require(ViewerActivityFixtures.runs(activity).first)
    #expect(run.allowance == nil)
    #expect(run.account == "github.com")
    #expect(run.steps.map(\.allowance?.words) == ["You allowed for this task", "You allowed once"])
  }

  @Test("a step whose words name only the browser still shows the account it acted in, beside them and to VoiceOver")
  func theAccountShowsWhateverTheWordsSay() throws {
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.click("c2", "clicked Merge in Google Chrome", label: "Merge", age: 3, allowed: .trusted),
      Self.click("c1", "clicked Search in Google Chrome", label: "Search", age: 5, account: nil),
    ])
    let activity = ViewerActivityFixtures.opened(snapshot)
    let step = try #require(ViewerActivityFixtures.steps(activity).last)
    #expect(step.words == ViewerStepWords("Clicked", target: "Merge"))
    #expect(step.account == "github.com")
    #expect(step.label == "Clicked Merge, github.com, Trusted")
  }

  @Test("a Trusted step beside one outside any listed account gives its run no account and no badge")
  func aRunPartlyOutsideHasNoBadge() throws {
    let snapshot = try ViewerActivityFixtures.onActivity([
      Self.click("c2", "clicked Search in Google Chrome", label: "Search", age: 3, account: nil),
      Self.click("c1", "clicked Merge on github.com", label: "Merge", age: 5, allowed: .trusted),
    ])
    let run = try #require(ViewerActivityFixtures.runs(ViewerActivityFixtures.opened(snapshot)).first)
    #expect(run.account == nil)
    #expect(run.allowance == nil)
    #expect(run.name == "Google Chrome")
    #expect(run.steps.first?.allowance?.words == "Trusted", "the step keeps its own")
  }

  @Test("a row that names its browser only by bundle counts toward the run, so another site keeps the browser's name")
  func aBundleOnlyRowCounts() throws {
    let gitlab = ActivityLog.Account(name: "gitlab.com", key: AccountKey(kind: .site, id: "gitlab.com"))
    let snapshot = try ViewerActivityFixtures.onActivity([
      ActivityLog.Entry(
        id: "c2",
        says: "clicked Close on gitlab.com",
        ageSeconds: 3,
        session: ViewerActivityFixtures.claude,
        verb: "click",
        label: "Close",
        bundleId: "com.google.Chrome",
        account: gitlab,
        allowed: .task
      ),
      Self.click("c1", "clicked Merge on github.com", label: "Merge", age: 5, allowed: .task),
    ])
    let run = try #require(ViewerActivityFixtures.runs(ViewerActivityFixtures.opened(snapshot)).first)
    #expect(run.steps.count == 2, "one run in Chrome")
    #expect(run.name == "Google Chrome", "never titled by one of its two sites")
    #expect(run.account == nil && run.allowance == nil)
    #expect(run.steps.map(\.account) == ["github.com", "gitlab.com"])
  }
}
