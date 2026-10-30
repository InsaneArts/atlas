import Foundation

@testable import AtlasApp

extension ViewerSnapshot {
  /// A presenter snapshot at 17:30 UTC on 29 October 2026, with a fixed German locale and Gregorian calendar.
  static var presenterSample: ViewerSnapshot {
    let locale = Locale(identifier: "de_DE")
    var calendar = Calendar(identifier: .gregorian)
    calendar.locale = locale
    calendar.timeZone = .gmt
    return ViewerSnapshot(
      timeZone: .gmt,
      locale: locale,
      date: Date(timeIntervalSince1970: 1_793_295_000),
      calendar: calendar
    )
  }
}
