#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  extension ViewerPreviewStatesTests {
    /// The Screen tab's samples' facts, by the sample's name: each top-row state, alert and empty message.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func screenSamples() throws -> [String: ViewerState] {
      let idle = try reported("observing", state: "idle")
      let loggedOut = try reported("loggedOut", state: "notReady")
      let gone = "Atlas isn't answering in the helper."
      return [
        "Working": shown(try reported("agent", state: "working", activity: Self.working)),
        "Idle": shown(idle),
        "You have control": shown(try reported("controlling", state: "control")) { $0.lease = "lease" },
        "Needs you": shown(try reported("recovering", state: "needsYou")),
        "Stopped": shown(try reported("stopped", state: "stopped")),
        "In use elsewhere": shown(try reported("controlling", state: "control")),
        "Paused": shown(idle) { $0.session.paused = true },
        "Details": shown(idle) {
          $0.detailsShown = true
          $0.agents = [.claude, .codex]
          // 09:12 in UTC, told as a German clock tells it, as the board does.
          $0.loggedInAt = Date(timeIntervalSince1970: 9 * 3600 + 12 * 60)
          $0.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
          $0.locale = Locale(identifier: "de_DE")
        },
        "Details, some checks not passing": shown(
          try reported(
            "observing",
            state: "idle",
            checks: [
              ["name": "Helper running", "ok": true],
              ["name": "Input service ready", "ok": false, "message": "Accessibility is off"],
            ],
            dialogs: ["state": "unchecked", "reason": "Not run since the helper started"]
          )
        ) {
          $0.detailsShown = true
          $0.agents = [.claude, .codex]
          $0.loggedInAt = Date(timeIntervalSince1970: 9 * 3600 + 12 * 60)
          $0.timeZone = TimeZone(secondsFromGMT: 0) ?? .current
          $0.locale = Locale(identifier: "de_DE")
        },
        "Not ready": shown(
          try reported("unavailable", state: "notReady", reason: nil),
          picture: false,
          unavailable: gone
        ),
        "Logged out": shown(loggedOut, picture: false, unavailable: gone),
        "Connecting": shown(idle, picture: false, unavailable: gone) {
          $0.reading.lastPicture = SampleDesktop.picture
          $0.reading.waitedLong = true
        },
        "Switch": shown(loggedOut, picture: false, unavailable: gone) { $0.session.alert = .switchToHelper },
        "Log out": shown(idle) { $0.session.alert = .logOut },
        "Log out anyway": shown(idle) { $0.session.alert = .logOutAnyway },
        "Restart": shown(idle) { $0.session.alert = .restart },
        "Typing": shown(try reported("controlling", state: "control")) {
          $0.lease = "lease"
          $0.composing = "こんにちは"
        },
        "Mid-action": shown(try reported("agent", state: "working", activity: Self.working)) {
          $0.session.toast = ViewerWords.agentMidAction
          $0.session.refusals = 1
        },
        "Actual size, paused": shown(idle) {
          $0.session.paused = true
          $0.session.actualSize = true
        },
        "Actual size, needs you": shown(try reported("recovering", state: "needsYou")) { $0.session.actualSize = true },
      ]
    }
  }
#endif
