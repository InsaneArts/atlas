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

    @Test("the panel previews show every state, picture, main action and notice")
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
        states.compactMap { state -> String? in
          switch state.picture {
          case .screen(let picture, let look, let caption):
            guard picture != nil else { return "empty stage" }
            return caption != nil ? "caption" : "\(look)"
          case .empty: return "empty"
          }
        }
      )
      #expect(looks == ["live", "caption", "inControl", "dimmed", "paused", "empty", "empty stage"])
      #expect(Set(states.map(\.controls.main.title)).count == 5, "Open Atlas, Give Back, Recover, Resume, Switch")
      #expect(states.contains { !$0.controls.main.isEnabled })
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
      #expect(states.contains { $0.top.actions.pause.isOn })
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
      #expect(states.contains { $0.screen.outline != nil && $0.top.actions.showsClear })
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
      #expect(
        Set(states.compactMap(\.about.answer)).count >= 5,
        "up to date, newer, failed, install failed, unpublished"
      )
      #expect(states.contains { $0.agents.projects.isEmpty }, "no project taught")
      #expect(states.contains { $0.helper.ready.contains { $0.item.state != .ready } }, "a check not ready")
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
            case .blocked: "blocked"
            }
          }
        }
      )
      #expect(
        shown == [
          "loading", "permissionPanel", "welcome", "choose", "create", "switchToHelper", "finishing", "agents", "done",
          "allow",
          "permissions", "helperDone", "blocked",
        ]
      )
      #expect(PreviewStates.onboarding.contains { $0.value.isBusy }, "a command running")
    }
  }
#endif
