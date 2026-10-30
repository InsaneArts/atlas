import Foundation

extension Words {
  /// A time of day to the second, on the person's own 12- or 24-hour clock, as the Activity and Screenshots tabs
  /// list it.
  ///
  /// - Parameters:
  ///   - date: The moment.
  ///   - locale: The person's locale, which picks the clock.
  ///   - calendar: The calendar supplied by the snapshot.
  ///   - timeZone: The person's time zone.
  /// - Returns: "10:31:42" on a 24-hour clock, "10:31:42 AM" on a 12-hour one.
  static func clockTime(_ date: Date, locale: Locale, calendar: Calendar, timeZone: TimeZone) -> String {
    let style = Date.FormatStyle(locale: locale, calendar: calendar, timeZone: timeZone)
      .hour(.twoDigits(amPM: .abbreviated))
      .minute(.twoDigits)
      .second(.twoDigits)
    return date.formatted(style)
  }

  /// A time of day to the minute, on the person's own 12- or 24-hour clock, as an Activity moment says when it started.
  ///
  /// - Parameters:
  ///   - date: The moment.
  ///   - locale: The person's locale, which picks the clock.
  ///   - calendar: The calendar supplied by the snapshot.
  ///   - timeZone: The person's time zone.
  /// - Returns: "10:31" on a 24-hour clock, "10:31 AM" on a 12-hour one.
  static func clockMinute(_ date: Date, locale: Locale, calendar: Calendar, timeZone: TimeZone) -> String {
    let style = Date.FormatStyle(locale: locale, calendar: calendar, timeZone: timeZone)
      .hour(.twoDigits(amPM: .abbreviated))
      .minute(.twoDigits)
    return date.formatted(style)
  }

  /// A day before yesterday, as a group's label.
  ///
  /// - Parameters:
  ///   - date: A moment of the day.
  ///   - locale: The person's locale.
  ///   - calendar: The calendar supplied by the snapshot.
  ///   - timeZone: The person's time zone.
  /// - Returns: "Mon 19 Oct", in the locale's own order.
  static func dayName(_ date: Date, locale: Locale, calendar: Calendar, timeZone: TimeZone) -> String {
    let style = Date.FormatStyle(locale: locale, calendar: calendar, timeZone: timeZone)
      .weekday(.abbreviated)
      .day()
      .month(.abbreviated)
    return date.formatted(style)
  }
}
