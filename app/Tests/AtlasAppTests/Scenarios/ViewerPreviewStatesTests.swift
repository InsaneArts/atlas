#if DEBUG
  import AtlasKit
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  /// The viewer's samples are written out by hand; these keep each one what the viewer would show for the same facts.
  @MainActor
  @Suite("PreviewStates+Viewer")
  struct ViewerPreviewStatesTests {
    /// A status as the viewer service reports it, with recent activity by an agent and by the person.
    ///
    /// - Parameters:
    ///   - mode: The helper's mode.
    ///   - reason: Why it is in that mode.
    ///   - ok: Whether the input service check passed and the latest action worked.
    /// - Returns: The decoded status.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func reported(_ mode: String, reason: String = "An agent is acting in the helper", ok: Bool = true) throws
      -> ViewerStatus
    {
      let reply: [String: Any] = [
        "ready": true,
        "canObserve": true,
        "checks": [["name": "Helper session", "ok": true], ["name": "Input service", "ok": ok]],
        "session": ["user": ["name": "Atlas"], "inputService": ["ok": true, "ready": true]],
        "capabilities": ["nativeDialogs": ["state": "unknown", "reason": "Run atlas verify-native to check."]],
        "overview": [
          "mode": mode,
          "reason": reason,
          "account": "atlas",
          "accountName": "Atlas",
          "display": ["width": 1512, "height": 982],
          "pending": mode == "recovering" ? 1 : 0,
          "activity": [
            ["operation": "input", "actor": "agent", "ok": mode != "recovering", "actionCount": 2, "ageSeconds": 2],
            ["operation": "run", "actor": "agent", "ok": true, "ageSeconds": 7],
            ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 5, "ageSeconds": 240],
          ],
        ],
      ]
      return try status(reply)
    }

    /// What the presenter shows for a status and the window's own facts.
    ///
    /// - Parameters:
    ///   - status: What the service reports.
    ///   - picture: Whether the helper's desktop shows.
    ///   - lease: The control lease the window holds, if any.
    ///   - paused: Whether the picture is paused.
    ///   - unavailable: Why there is no picture, if there is none.
    ///   - sidebar: Whether the sidebar is open.
    ///   - notice: A line about what just happened.
    ///   - composing: Text an input method is composing.
    ///   - canLogIn: Whether Log In… can be offered.
    /// - Returns: The state.
    func shown(
      _ status: ViewerStatus,
      picture: Bool = true,
      lease: String? = nil,
      paused: Bool = false,
      unavailable: String? = nil,
      sidebar: Bool = false,
      notice: String? = nil,
      composing: String = "",
      canLogIn: Bool = false
    ) -> ViewerState {
      var snapshot = ViewerSnapshot()
      snapshot.reading.status = status
      snapshot.reading.hasStatus = true
      snapshot.reading.ready = true
      snapshot.reading.unavailable = unavailable
      if picture, let sample = SampleDesktop.picture {
        snapshot.reading.picture = sample
        snapshot.reading.frame = Frame(
          png: Data(),
          id: sample.id,
          revision: sample.id,
          display: ViewerStatus.Size(width: 1512, height: 982)
        )
        snapshot.reading.frameAt = snapshot.now
      }
      snapshot.session.lease = lease
      snapshot.session.paused = paused
      snapshot.session.notice = notice
      snapshot.session.composing = composing
      snapshot.sidebarShown = sidebar
      snapshot.canLogIn = canLogIn
      return ViewerPresenter.state(snapshot)
    }

    @Test("each viewer sample is what the viewer shows for the same facts")
    func samplesMatchThePresenter() throws {
      let presented: [String: ViewerState] = [
        "Watching": shown(try reported("agent")),
        "Sidebar": shown(try reported("observing", reason: "Watching the helper"), sidebar: true),
        "Sidebar and control": shown(
          try reported("controlling", reason: "You have control"),
          lease: "lease",
          sidebar: true
        ),
        "Control": shown(try reported("controlling", reason: "A viewer has control"), lease: "lease"),
        "Recovery": shown(
          try reported("recovering", reason: "Atlas can't tell whether the last action worked.", ok: false)
        ),
        "Paused": shown(try reported("observing"), paused: true),
        "Paused before a picture": shown(try reported("observing"), picture: false, paused: true),
        "Unavailable": shown(
          try reported("unavailable", reason: "Atlas isn't answering in the helper"),
          picture: false,
          unavailable: "Atlas isn't answering in the helper. Open Getting Started if this lasts."
        ),
        "Notice": shown(try reported("observing"), notice: "Control released."),
        "Logged out": shown(
          try reported("loggedOut", reason: "The helper is logged out, so agents can't use it"),
          picture: false,
          unavailable: "The helper is logged out, so agents can't use it",
          canLogIn: true
        ),
        "Connecting": ViewerPresenter.state(ViewerSnapshot()),
        "Typing": shown(try reported("controlling", reason: "You have control"), lease: "lease", composing: "こんにちは"),
      ]
      #expect(Set(presented.keys) == Set(PreviewStates.viewer.map(\.name)), "every sample has its facts, and no more")
      for preview in PreviewStates.viewer {
        let state = try #require(presented[preview.name], "\(preview.name) has no facts")
        #expect(preview.value == state, "\(preview.name)")
      }
    }
  }
#endif
