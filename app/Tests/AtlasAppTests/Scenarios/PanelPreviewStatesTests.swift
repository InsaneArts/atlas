#if DEBUG
  import AtlasKit
  import AtlasUI
  import Testing

  @testable import AtlasApp

  /// The panel's samples are written out by hand; these keep each one what the panel would show for the same facts.
  @MainActor
  @Suite("PreviewStates+Panel")
  struct PanelPreviewStatesTests {
    /// What the controller holds on a set-up Mac whose helper is in `mode`: Atlas, its display and an agent's click
    /// 7 seconds ago, its desktop on show, its helper known and Open at Login off.
    ///
    /// - Parameters:
    ///   - mode: The helper's mode, as the service names it.
    ///   - reason: Why it is in that mode.
    /// - Returns: The snapshot.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func snapshot(_ mode: String, reason: String = "Ready for agents") throws -> PanelSnapshot {
      let reported = try status([
        "overview": [
          "mode": mode,
          "reason": reason,
          "account": "atlas",
          "accountName": "Atlas",
          "display": ["width": 1512, "height": 982],
          "activity": [["operation": "click", "actor": "agent", "ok": true, "ageSeconds": 7]],
        ]
      ])
      return PanelSnapshot(
        status: reported,
        picture: SampleDesktop.picture,
        helper: "atlas",
        opensAtLogin: OpenAtLogin.State(isAvailable: true, isEnabled: false)
      )
    }

    @Test("each panel sample is what the panel shows for the same facts")
    func samplesMatchThePresenter() throws {
      var noPicture = try snapshot("observing")
      noPicture.picture = nil
      noPicture.placeholder = PanelWords.pictureUnavailable
      var confirming = try snapshot("observing")
      confirming.logOutStage = .confirming
      var held = try snapshot("observing")
      held.logOutStage = .held
      held.feedback = "The helper has not logged out yet"
      var working = try snapshot("observing")
      working.working = true
      working.feedback = Words.recovering
      var unavailable = PanelSnapshot(
        status: try status(["overview": ["mode": "unavailable", "reason": Words.connecting]]),
        placeholder: PanelWords.pictureUnavailable,
        opensAtLogin: OpenAtLogin.State(isAvailable: true, isEnabled: false)
      )
      unavailable.helper = nil
      let presented: [String: PanelSnapshot] = [
        "Watching": try snapshot("observing"),
        "Agent working": try snapshot("agent", reason: "An agent is acting in the helper"),
        "In control": try snapshot("controlling", reason: "A viewer has control"),
        "Recovering": try snapshot(
          "recovering",
          reason: "An action's outcome is unknown. Recover the helper before anything else runs."
        ),
        "Logged out": try snapshot("loggedOut", reason: "The helper is logged out"),
        "No picture": noPicture,
        "Confirm log-out": confirming,
        "Log-out held": held,
        "Working message": working,
        "Unavailable": unavailable,
      ]
      #expect(Set(presented.keys) == Set(PreviewStates.panel.map(\.name)), "every sample has its facts, and no more")
      for preview in PreviewStates.panel {
        let snapshot = try #require(presented[preview.name], "\(preview.name) has no facts")
        #expect(preview.value == PanelPresenter.state(snapshot), "\(preview.name)")
      }
    }
  }
#endif
