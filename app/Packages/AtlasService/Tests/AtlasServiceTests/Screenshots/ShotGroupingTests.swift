import Foundation
import Testing

@testable import AtlasService

@Suite("ShotGrouping")
struct ShotGroupingTests {
  /// 11:13:00 on 22 October 2026, in UTC.
  static let now = Date(timeIntervalSince1970: 1_792_667_580)

  /// A Gregorian calendar in UTC, so a day and an hour are the same on any Mac.
  static let calendar: Calendar = {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
    return calendar
  }()

  /// A moment on 22 October 2026, or a day before it, in UTC.
  ///
  /// - Parameters:
  ///   - hour: The hour.
  ///   - minute: The minute.
  ///   - daysBefore: How many days before the 22nd.
  /// - Returns: The moment.
  static func at(_ hour: Int, _ minute: Int, daysBefore: Int = 0) -> Date {
    now.addingTimeInterval(Double(((hour - 11) * 60 + minute - 13) * 60 - daysBefore * 86_400))
  }

  @Test(
    "screenshots group by the last hour, the part of today they were taken in, yesterday and the day",
    arguments: [
      (at(10, 31), ShotPeriod.lastHour),
      (at(10, 14), .lastHour),
      (at(10, 12), .morning),
      (at(11, 30), .lastHour),
      (at(13, 0, daysBefore: 1), .yesterday),
    ] as [(Date, ShotPeriod)]
  )
  func periods(takenAt: Date, period: ShotPeriod) {
    #expect(ShotGrouping(now: Self.now, calendar: Self.calendar).period(of: takenAt) == period)
  }

  @Test("earlier today reads as the afternoon from noon and the evening from six")
  func partsOfTheDay() {
    let evening = Self.now
      .addingTimeInterval(10 * 3600)
    let rules = ShotGrouping(now: evening, calendar: Self.calendar)
    #expect(rules.period(of: Self.at(11, 59)) == .morning)
    #expect(rules.period(of: Self.at(12, 0)) == .afternoon)
    #expect(rules.period(of: Self.at(18, 0)) == .evening)
    let older = Self.at(9, 0, daysBefore: 3)
    #expect(rules.period(of: older) == .day(Self.calendar.startOfDay(for: older)))
  }

  @Test("search by time finds 10:31 in 10:31:44")
  func searchByTime() {
    let shots = [
      Screenshot(name: "a-capture.png", takenAt: Self.at(10, 31).addingTimeInterval(44)),
      Screenshot(name: "b-capture.png", takenAt: Self.at(9, 58).addingTimeInterval(41)),
    ]
    let days = ShotGrouping(now: Self.now, calendar: Self.calendar)
      .days(shots, matching: "10:31") { date in
        let style = Date.FormatStyle(locale: Locale(identifier: "de_DE"), timeZone: Self.calendar.timeZone)
          .hour(.twoDigits(amPM: .abbreviated))
          .minute(.twoDigits)
          .second(.twoDigits)
        return date.formatted(style)
      }
    #expect(days.flatMap(\.shots).map(\.name) == ["a-capture.png"])
  }

  @Test("Next stops at the last screenshot, and Previous at the first")
  func nextStopsAtTheLast() {
    let order = ["newest", "middle", "oldest"]
    #expect(ShotGrouping.step(from: "middle", by: 1, in: order) == "oldest")
    #expect(ShotGrouping.step(from: "oldest", by: 1, in: order) == nil)
    #expect(ShotGrouping.step(from: "newest", by: -1, in: order) == nil)
  }

  @Test("screenshots list newest first whatever order they come in")
  func newestFirst() {
    let shots = [
      Screenshot(name: "older-capture.png", takenAt: Self.at(10, 31).addingTimeInterval(20)),
      Screenshot(name: "newer-capture.png", takenAt: Self.at(10, 31).addingTimeInterval(45)),
    ]
    let names = ShotGrouping(now: Self.now, calendar: Self.calendar)
      .days(shots, matching: "") { _ in "" }
      .flatMap(\.shots)
      .map(\.name)
    #expect(names == ["newer-capture.png", "older-capture.png"])
  }
}
