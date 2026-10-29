#if DEBUG
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("PreviewStates")
  struct PreviewStatesTests {
    @Test("preview names are unique")
    func namesAreUnique() {
      for names in [
        PreviewStates.panel.map(\.name), PreviewStates.onboarding.map(\.name), PreviewStates.viewer.map(\.name),
        PreviewStates.settings.map(\.name), PreviewStates.alert.map(\.name),
      ] {
        #expect(Set(names).count == names.count)
      }
    }

    @Test("the panel previews show every state, picture look, main action and notice")
    func panelPreviewsCoverEveryPiece() {
      let states = PreviewStates.panel
        .map(\.value)
      #expect(
        Set(states.map(\.status.label)) == [
          "Idle", "Working", "You have control", "Needs you", "Stopped", "Logged out", "Not ready", "Connecting",
          "In use elsewhere",
        ]
      )
      let looks = Set(
        states.map { state -> String in
          switch state.picture.look {
          case .empty: "empty"
          default: "\(state.picture.look)"
          }
        }
      )
      #expect(looks == ["live", "control", "needsYou", "paused", "empty", "connecting"])
      #expect(states.contains { $0.picture.caption != nil }, "the newest action over the screen")
      #expect(states.contains { $0.recent.contains(where: \.isLive) }, "a live row in Recent")
      #expect(states.contains { $0.picture.pause.isPaused }, "the pause button showing play")
      #expect(Set(states.map(\.controls.main.title)).count == 5, "Open Atlas, Give Back, Recover, Switch, Settings")
      #expect(states.allSatisfy { $0.controls.main.isEnabled }, "no main action is ever greyed")
      #expect(states.contains { $0.controls.agent == .stop }, "Stop while an agent works")
      #expect(states.contains { $0.controls.agent == .resume(isEnabled: true) }, "Resume once agents are stopped")
      #expect(states.contains { $0.notice != nil })
      #expect(states.contains { $0.recent.isEmpty } && states.contains { !$0.recent.isEmpty })
    }

    @Test("the alert window previews show every alert")
    func alertPreviewsCoverEveryAlert() {
      #expect(Set(PreviewStates.alert.map(\.value.title)).count == 4, "Recover, Switch, Log Out, Log Out Anyway")
    }

    @Test("the main window previews show every state the header says, alert, empty message, footer line and list")
    func viewerPreviewsCoverEveryPiece() {
      let states = PreviewStates.viewer
        .map(\.value)
      #expect(
        Set(states.map(\.header.who)).isSuperset(of: [
          "Working", "Idle", "You", "Needs you", "Stopped", "In use elsewhere", "Not ready", "Logged out", "Connecting",
          "Paused", SessionSamples.claudeName,
        ])
      )
      #expect(
        Set(states.compactMap { $0.alert?.title }).count == 6,
        "Recover, Switch, Log Out, Log Out Anyway, Restart, Clean Up"
      )
      #expect(Set(states.compactMap { $0.screen.picture == nil ? $0.screen.empty?.message.headline : nil }).count == 3)
      let lines = Set(
        states.map { state in
          let latest = state.footer.latest
          if latest.caret { return "typing" }
          if latest.announces { return latest.tone == .attention ? "refusal" : "notice" }
          if latest.tone == .link { return "control" }
          return latest.age.isEmpty ? "state" : latest.icon == nil ? "action" : "session action"
        }
      )
      #expect(lines == ["typing", "refusal", "notice", "control", "state", "action", "session action"])
      let sections = Set(states.flatMap { $0.sidebar.sessions.groups.map(\.section) })
      #expect(sections == Set(ViewerSessionSection.allCases), "every section, Needs you and Ended included")
      #expect(states.contains { $0.sidebar.sessions.endedShown })
      #expect(states.contains { $0.sidebar.sessions.endedCount > 0 && !$0.sidebar.sessions.endedShown })
      #expect(states.contains { $0.sidebar.search.isOpen })
      #expect(Set(states.map(\.sidebar.mode)) == [.full, .hidden])
      let rows = states.flatMap { $0.sidebar.sessions.groups.flatMap(\.rows) }
      #expect(rows.contains { $0.isCleaningUp }, "a session's work being cleaned up")
      #expect(rows.contains { $0.help.contains("Last cleanup") }, "a session's last cleanup in its tooltip")
      #expect(
        states.contains { $0.sidebar.helper.words.hasPrefix("Helper needs setup ·") },
        "a check that does not pass, counted on the helper's line"
      )
      #expect(states.contains { $0.footer.controls.pause?.isOn == true })
      #expect(states.contains { $0.footer.controls.control?.look == .accent })
      #expect(states.contains { $0.footer.controls.agent == .resume(isEnabled: true) })
      let panes = Set(
        states.map { state in
          switch state.pane {
          case .screen: "screen"
          case .activity(let activity): activity.empty == nil ? "activity" : "activity, empty"
          case .apps: "apps"
          case .screenshots(let shots): shots.large != nil ? "large" : shots.empty == nil ? "shots" : "shots, empty"
          }
        }
      )
      #expect(panes == ["screen", "activity", "activity, empty", "apps", "shots", "large", "shots, empty"])
      #expect(states.contains { $0.screen.outline != nil })
    }

    @Test("the settings previews show every pane, alert and update step")
    func settingsPreviewsCoverEveryPiece() {
      let states = PreviewStates.settings
        .map(\.value)
      #expect(Set(states.map(\.tabs.selected)) == Set(SettingsTab.allCases))
      #expect(
        Set(states.compactMap { $0.alert?.title }).count == 6,
        "Recover, Switch, Log Out, Log Out Anyway, Forget, Install"
      )
      #expect(states.contains { $0.alert?.isBusy == true }, "an alert whose command runs")
      let updates = Set(
        states.map { state in
          switch state.about.update {
          case .check: "check"
          case .checking: "checking"
          case .install: "install"
          case .installing: "installing"
          }
        }
      )
      #expect(updates == ["check", "checking", "install", "installing"])
      let statuses = Set(
        states.compactMap { state in
          switch state.about.status {
          case .working: "working"
          case .done: "done"
          case .available: "available"
          case .installing: "installing"
          case .problem: "problem"
          case .misplaced: "misplaced"
          case .note: "note"
          case nil: nil
          }
        }
      )
      #expect(statuses == ["working", "done", "available", "installing", "problem", "misplaced", "note"])
      #expect(states.contains { $0.helper.waiting != nil }, "the buttons waiting on another action")
      #expect(states.contains { $0.helper.readyNote != nil }, "what Check Again found")
      #expect(states.contains { $0.helper.ready.contains(where: \.offersRecover) }, "Recover… on a failing check")
      #expect(states.contains { $0.agents.agents.contains { $0.problem != nil } }, "a Connect that failed")
      #expect(states.contains { $0.agents.agents.contains { $0.connection == .missing } }, "an agent not installed")
      #expect(states.contains { $0.agents.projects.isEmpty }, "no project taught")
      #expect(states.contains { $0.helper.ready.contains { $0.item.state != .ready } }, "a check not ready")
    }

    @Test("setup and Settings previews use original agent logos")
    func agentPreviewsUseLogos() {
      for preview in PreviewStates.settings {
        #expect(preview.value.agents.agents.map(\.id) == ["claude", "codex", "opencode", "pi", "cursor"])
        #expect(preview.value.agents.agents.first?.title == "Claude")
        for agent in preview.value.agents.agents {
          let expected: RowIcon =
            agent.id == "cursor"
            ? .app(AgentLogos.cursor)
            : agent.id == "pi"
              ? .mark(.pi)
              : agent.id == "opencode"
                ? .mark(.opencode) : .app(agent.id == "codex" ? AgentLogos.codex : AgentLogos.claude)
          #expect(agent.icon == expected)
        }
      }
      for preview in PreviewStates.onboarding {
        guard case .screen(.agents(let screen)) = preview.value.content else { continue }
        #expect(screen.agents.map(\.id) == ["claude", "codex", "opencode", "pi", "cursor"])
        #expect(screen.agents.first?.title == "Claude")
        for agent in screen.agents {
          let expected: RowIcon =
            agent.id == "cursor"
            ? .app(AgentLogos.cursor)
            : agent.id == "pi"
              ? .mark(.pi)
              : agent.id == "opencode"
                ? .mark(.opencode) : .app(agent.id == "codex" ? AgentLogos.codex : AgentLogos.claude)
          #expect(agent.icon == expected)
        }
      }
    }

    @Test("the onboarding previews show every screen")
    func onboardingPreviewsCoverEveryScreen() {
      let shown = Set(
        PreviewStates.onboarding.map { preview in
          switch preview.value.content {
          case .loading: "loading"
          case .permissionPanel: "permissionPanel"
          case .screen(let screen):
            switch screen.id {
            case .welcome: "welcome"
            case .choose: "choose"
            case .create: "create"
            case .switchToHelper: "switchToHelper"
            case .finishing: "finishing"
            case .agents: "agents"
            case .done: "done"
            case .allow: "allow"
            case .permissions: "permissions"
            case .helperDone: "helperDone"
            case .closeLeftovers: "closeLeftovers"
            case .blocked: "blocked"
            }
          }
        }
      )
      #expect(
        shown == [
          "loading", "permissionPanel", "welcome", "choose", "create", "switchToHelper", "finishing", "agents", "done",
          "allow",
          "permissions", "helperDone", "blocked", "closeLeftovers",
        ]
      )
      let notes = PreviewStates.onboarding
        .map(\.value.note)
      let running = notes.contains { note in
        if case .working = note { true } else { false }
      }
      #expect(running, "a command running")
    }
  }
#endif
