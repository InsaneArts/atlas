import AtlasCommands
import AtlasUI
import Foundation

/// What the main window's controller holds, as one plain value for ``ViewerPresenter``.
struct ViewerSnapshot {
  /// What the last reading showed.
  var reading = ViewerReading()
  /// What the window is doing.
  var session = ViewerSession()
  /// Whether the person asked for Details.
  var detailsShown = false
  /// How often a picture is read while watching.
  var rate = ViewerPictureRate.live
  /// The time the window last read the clock, in seconds of uptime.
  var now: TimeInterval = 0
  /// The helper's commands running now, from any window.
  var running: Set<HelperActions.Command> = []
  /// Stop and Resume requests unanswered from any window, so Resume waits.
  var agentRequests = 0
  /// Whether control work is under way anywhere in the app: an acquisition awaiting its reply, or a release.
  var controlBusy = false
  /// The agents connected to Atlas, as last read for Details; nil when unknown.
  var agents: [AgentID]?
  /// When the helper's account logged in; nil when unknown.
  var loggedInAt: Date?
  /// The time zone Details tells the login time in.
  var timeZone = TimeZone.current
  /// The locale Details tells the login time in, and the tabs their times and days.
  var locale = Locale.current
  /// What the Activity, Apps and Screenshots tabs hold.
  var panes = ViewerPanes()
  /// The date and time now, which the tabs tell Now, today and the last hour by.
  var date = Date()
  /// The calendar the tabs tell days and hours by; its time zone is ``timeZone``'s.
  var calendar = Calendar(identifier: .gregorian)

  /// ``calendar`` in ``timeZone``.
  var zonedCalendar: Calendar {
    var zoned = calendar
    zoned.timeZone = timeZone
    return zoned
  }
}
