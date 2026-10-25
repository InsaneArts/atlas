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

    @Test("the main window previews show every top-row state, alert, empty message, chip and tab")
    func viewerPreviewsCoverEveryPiece() {
      let states = PreviewStates.viewer
        .map(\.value)
      #expect(
        Set(states.map(\.top.status.label)) == [
          "Working", "Idle", "You have control", "Needs you", "Stopped", "In use elsewhere", "Not ready", "Logged out",
          "Connecting",
        ]
      )
      #expect(
        Set(states.compactMap { $0.alert?.title }).count == 5,
        "Recover, Switch, Log Out, Log Out Anyway, Restart"
      )
      #expect(Set(states.compactMap { $0.screen.picture == nil ? $0.screen.empty?.message.headline : nil }).count == 3)
      let chips = Set(
        states.compactMap { state in
          state.screen.chip.map { chip in
            switch chip {
            case .typing: "typing"
            case .inControl: "inControl"
            case .paused: "paused"
            }
          }
        }
      )
      #expect(chips == ["typing", "inControl", "paused"])
      #expect(states.contains { $0.details != nil })
      #expect(states.contains { $0.top.actions.pause?.isOn == true })
      #expect(states.contains { $0.toast != nil })
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

    @Test("setup and Settings previews and renders use the bundled original logos for every agent")
    func agentPreviewsUseLogos() {
      for preview in PreviewStates.settings {
        #expect(preview.value.agents.agents.map(\.id) == ["claude", "codex", "pi"])
        #expect(preview.value.agents.agents.first?.title == "Claude")
        for agent in preview.value.agents.agents {
          let expected: RowIcon =
            agent.id == "pi"
            ? .mark(.pi)
            : .app(agent.id == "codex" ? AgentLogos.codex : AgentLogos.claude)
          #expect(agent.icon == expected)
        }
      }
      for preview in PreviewStates.onboarding {
        guard case .screen(.agents(let screen)) = preview.value.content else { continue }
        #expect(screen.agents.map(\.id) == ["claude", "codex", "pi"])
        #expect(screen.agents.first?.title == "Claude")
        for agent in screen.agents {
          let expected: RowIcon =
            agent.id == "pi"
            ? .mark(.pi)
            : .app(agent.id == "codex" ? AgentLogos.codex : AgentLogos.claude)
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
