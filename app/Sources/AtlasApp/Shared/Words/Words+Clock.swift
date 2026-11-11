import Foundation

extension Words {
  /// A time of day to the second, on the person's own 12- or 24-hour clock, as Activity and the picture sheet say
  /// it.
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

  /// A time of day to the minute, on the person's own 12- or 24-hour clock, as a replay and a session's title say
  /// when it started.
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

  /// 6: the days before today that ``shortDay(_:now:locale:calendar:timeZone:)`` names by their weekday alone.
  static let weekdayDays = 6

  /// When something happened, as short as a list's trailing edge says it: the time of day today, the short weekday
  /// alone within the last ``weekdayDays`` days, else the short month and day.
  ///
  /// - Parameters:
  ///   - date: The moment.
  ///   - now: The time it is now.
  ///   - locale: The person's locale.
  ///   - calendar: The calendar supplied by the snapshot.
  ///   - timeZone: The person's time zone.
  /// - Returns: "12:46", "Wed" or "Nov 8", in the locale's own order.
  static func shortDay(_ date: Date, now: Date, locale: Locale, calendar: Calendar, timeZone: TimeZone) -> String {
    let start = calendar.startOfDay(for: date)
    let days = calendar.dateComponents([.day], from: start, to: calendar.startOfDay(for: now)).day
    let style = Date.FormatStyle(locale: locale, calendar: calendar, timeZone: timeZone)
    switch days {
    case 0: return clockMinute(date, locale: locale, calendar: calendar, timeZone: timeZone)
    case .some(1...weekdayDays): return date.formatted(style.weekday(.abbreviated))
    default:
      return date.formatted(
        style.month(.abbreviated)
          .day()
      )
    }
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
