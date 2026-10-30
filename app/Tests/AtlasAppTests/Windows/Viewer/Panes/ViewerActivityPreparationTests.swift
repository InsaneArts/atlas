import AtlasService
import AtlasUI
import Foundation
import Testing

@testable import AtlasApp

@Suite("Prepared Viewer Activity")
struct ViewerActivityPreparationTests {
  @Test("presenting a copy of a snapshot never changes the original's moments")
  func presentingACopyKeepsTheOriginalMoments() throws {
    var snapshot = try ViewerPresenterActivityTests.onActivity(ViewerPresenterActivityTests.textEditLog, sessions: [])
    snapshot.panes.activity.preparation = try Self.prepared(snapshot)
    let moments = snapshot.panes.activity.moments
    let shown = ViewerPresenter.activity(snapshot)
    var copy = snapshot
    copy.panes.activity.log?.entries.removeAll()

    #expect(ViewerPresenter.activity(copy).days.isEmpty)
    #expect(snapshot.panes.activity.moments == moments)
    #expect(ViewerPresenter.activity(snapshot) == shown)
    #expect(ViewerPresenter.activity(copy).days.isEmpty)
    #expect(snapshot.panes.activity.moments == moments)
  }

  @Test("the same snapshot gives the same moment identities whatever was presented before")
  func presentationOrderKeepsMomentIdentities() throws {
    let rows = ViewerPresenterActivityTests.textEditLog
    let expected = ViewerPresenter.activity(try ViewerPresenterActivityTests.onActivity(rows, sessions: []))
    let snapshot = try ViewerPresenterActivityTests.onActivity(rows, sessions: [])
    var earlier = snapshot
    earlier.panes.activity.log?.entries.append(
      ViewerPresenterActivityTests.row("earlier", "opened TextEdit", verb: "open", age: 30)
    )
    let earlierIDs = ViewerPresenterActivityTests.moments(ViewerPresenter.activity(earlier))
      .map(\.id)
    let expectedIDs = ViewerPresenterActivityTests.moments(expected)
      .map(\.id)
    #expect(earlierIDs != expectedIDs, "the preceding projection has a different oldest action")
    #expect(ViewerPresenterActivityTests.moments(ViewerPresenter.activity(snapshot)).map(\.id) == expectedIDs)
    #expect(ViewerPresenterActivityTests.moments(ViewerPresenter.activity(earlier)).map(\.id) == earlierIDs)
    #expect(ViewerPresenterActivityTests.moments(ViewerPresenter.activity(snapshot)).map(\.id) == expectedIDs)
  }

  @Test(
    "Activity headings and clocks use the snapshot locale calendar and time zone",
    arguments: [
      ("en_US", "America/Los_Angeles", "Tue, Jmd.\u{202f}I 8", "03:31\u{202f}AM"),
      ("de_DE", "Asia/Tokyo", "Di. 8. Jmd.\u{202f}I", "19:31"),
    ]
  )
  func presentationUsesSnapshotContext(locale: String, timeZone: String, heading: String, clock: String) throws {
    let rows = [ViewerPresenterActivityTests.row("old", "opened TextEdit", verb: "open", age: 172_800)]
    var snapshot = try ViewerPresenterActivityTests.onActivity(rows, sessions: [])
    snapshot.locale = Locale(identifier: locale)
    snapshot.timeZone = try #require(TimeZone(identifier: timeZone))
    snapshot.calendar = Calendar(identifier: .islamicCivil)
    let activity = ViewerPresenter.activity(snapshot)
    #expect(activity.days.first?.title == heading)
    #expect(activity.days.first?.moments.first?.time == clock)
    #expect(activity.days.first?.moments.first?.steps.first?.time == clock)
  }

  @Test("an icon arriving doesn't reform Activity's moments")
  func iconsReusePreparedMoments() throws {
    var snapshot = try ViewerPresenterActivityTests.onActivity(ViewerPresenterActivityTests.textEditLog, sessions: [])
    let prepared = try Self.prepared(snapshot)
    snapshot.panes.activity.preparation = prepared
    let before = ViewerPresenter.activity(snapshot)
    let icon = try #require(ViewerPresenterAppIconsTests.icon)
    snapshot.panes.icons.images[AppIconKey.name("textedit").text] = icon
    let after = ViewerPresenter.activity(snapshot)
    #expect(try Self.prepared(snapshot).moments.moments == prepared.moments.moments)
    #expect(after.days.first?.moments.first?.icon == .app(.image(icon)))
    #expect(before.days.first?.moments.first?.icon != after.days.first?.moments.first?.icon)
    #expect(after.days.first?.moments.first?.steps == before.days.first?.moments.first?.steps)
  }

  @Test("time driven folds and age words update without reforming moments")
  func foldsUseTheCurrentTime() throws {
    let rows = [
      ViewerPresenterActivityTests.row("new", "opened Notes", verb: "open", app: "Notes", age: 1),
      ViewerPresenterActivityTests.row("older", "opened TextEdit", verb: "open", age: 299),
    ]
    var snapshot = try ViewerPresenterActivityTests.onActivity(rows, sessions: [])
    let prepared = try Self.prepared(snapshot)
    snapshot.panes.activity.preparation = prepared
    let before = ViewerPresenter.activity(snapshot)
    #expect(before.days[0].moments[1].isOpen)
    snapshot.date = snapshot.date.addingTimeInterval(1)
    let after = ViewerPresenter.activity(snapshot)
    #expect(!after.days[0].moments[1].isOpen, "the second moment folds at exactly five minutes")
    #expect(after.days[0].moments[1].age == "5 min")
    #expect(try Self.prepared(snapshot).moments.moments == prepared.moments.moments)
    snapshot.panes.activity.choices[after.days[0].moments[1].id] = true
    #expect(ViewerPresenter.activity(snapshot).days[0].moments[1].isOpen)
    #expect(try Self.prepared(snapshot).moments.moments == prepared.moments.moments)
  }

  @Test("scope search filters and status changes reuse prepared moments")
  func projectionsReuseThePreparation() throws {
    var snapshot = try ViewerPresenterActivityTests.onActivity(ViewerPresenterActivityTests.mixedLog)
    let prepared = try Self.prepared(snapshot)
    snapshot.panes.activity.preparation = prepared
    snapshot.session.chosenSession = .one(ViewerPresenterActivityTests.claude)
    #expect(ViewerPresenter.activity(snapshot).meta == "6 actions today")
    snapshot.session.search.query = "no match"
    #expect(ViewerPresenter.activity(snapshot).days.isEmpty)
    snapshot.panes.activity.filter = .problems
    snapshot.reading.hasStatus = false
    _ = ViewerPresenter.activity(snapshot)
    #expect(try Self.prepared(snapshot).moments.moments == prepared.moments.moments)
  }

  @Test("log captures calendar and midnight changes each replace the preparation")
  func changedInputsReplaceThePreparation() throws {
    var snapshot = try ViewerPresenterActivityTests.onActivity(ViewerPresenterActivityTests.textEditLog, sessions: [])
    let initial = try Self.prepared(snapshot)
    snapshot.panes.activity.preparation = initial
    snapshot.panes.activity.log?.entries.removeLast()
    let logChanged = try Self.prepared(snapshot)
    #expect(logChanged.log != initial.log)
    snapshot.panes.shots.shots = [Screenshot(name: "capture.png")]
    let capturesChanged = try Self.prepared(snapshot)
    #expect(capturesChanged.captures != logChanged.captures)
    snapshot.timeZone = try #require(TimeZone(secondsFromGMT: 3600))
    let calendarChanged = try Self.prepared(snapshot)
    #expect(calendarChanged.calendar != capturesChanged.calendar)
    snapshot.date = snapshot.date.addingTimeInterval(86_400)
    let nextDay = try Self.prepared(snapshot)
    #expect(nextDay.today != calendarChanged.today)
    #expect(ViewerPresenter.activity(snapshot).meta == "No actions today")
    #expect(ViewerPresenter.activity(snapshot).days[0].title == ViewerWords.yesterdayGroup)
  }

  @Test("a new reading time updates relative ages while an unchanged reading reuses preparation")
  func readingTimeAnchorsRelativeAges() throws {
    var snapshot = try ViewerPresenterActivityTests.onActivity(ViewerPresenterActivityTests.textEditLog, sessions: [])
    let first = try Self.prepared(snapshot)
    snapshot.panes.activity.preparation = first
    #expect(try Self.prepared(snapshot).moments.moments == first.moments.moments)
    let started = first.moments.moments.first?.startedAt
    snapshot.panes.activity.readAt = snapshot.date.addingTimeInterval(2)
    let next = try Self.prepared(snapshot)
    #expect(next.readAt != first.readAt)
    #expect(next.moments.moments.first?.startedAt == started?.addingTimeInterval(2))
  }

  /// Gets the same preparation the presenter reads for a snapshot.
  ///
  /// - Parameter snapshot: The Activity inputs.
  /// - Returns: The prepared reading.
  /// - Throws: A failed requirement when no log was read.
  private static func prepared(_ snapshot: ViewerSnapshot) throws -> ViewerActivityPreparation {
    try #require(
      snapshot.panes.activity.prepared(
        captures: snapshot.panes.shots.shots ?? [],
        calendar: snapshot.zonedCalendar,
        now: snapshot.date
      )
    )
  }
}
