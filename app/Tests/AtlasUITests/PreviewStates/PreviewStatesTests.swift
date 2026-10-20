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
      ] {
        #expect(Set(names).count == names.count)
      }
    }

    @Test("the panel previews show every stage and log-out step")
    func panelPreviewsCoverEveryPiece() {
      let states = PreviewStates.panel
        .map(\.value)
      let stages = Set(
        states.map { state in
          switch state.stage {
          case .picture: "picture"
          case .placeholder: "placeholder"
          case .loggedOut: "loggedOut"
          }
        }
      )
      #expect(stages == ["picture", "placeholder", "loggedOut"])
      let steps = Set(
        states.map { state in
          switch state.logOut {
          case .none: "none"
          case .confirming: "confirming"
          case .held: "held"
          }
        }
      )
      #expect(steps == ["none", "confirming", "held"])
      #expect(states.contains { $0.recovery != nil })
      #expect(states.contains { $0.feedback?.isWorking == true })
    }

    @Test("the viewer previews show every placeholder and chip")
    func viewerPreviewsCoverEveryPiece() {
      let stages = PreviewStates.viewer
        .map(\.value.stage)
      let placeholders = Set(
        stages.compactMap { stage in
          stage.placeholder.map { placeholder in
            switch placeholder {
            case .loggedOut: "loggedOut"
            case .unavailable: "unavailable"
            case .paused: "paused"
            case .connecting: "connecting"
            }
          }
        }
      )
      #expect(placeholders == ["loggedOut", "unavailable", "paused", "connecting"])
      let chips = Set(
        stages.compactMap { stage in
          stage.chip.map { chip in
            switch chip {
            case .typing: "typing"
            case .inControl: "inControl"
            case .paused: "paused"
            }
          }
        }
      )
      #expect(chips == ["typing", "inControl", "paused"])
    }

    @Test("the onboarding previews show every screen")
    func onboardingPreviewsCoverEveryScreen() {
      let shown = Set(
        PreviewStates.onboarding.map { preview in
          switch preview.value.content {
          case .loading: "loading"
          case .guide: "guide"
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
          "loading", "guide", "welcome", "choose", "create", "switchToHelper", "finishing", "agents", "done", "allow",
          "permissions", "helperDone", "blocked",
        ]
      )
      #expect(PreviewStates.onboarding.contains { $0.value.isBusy }, "a command running")
    }
  }
#endif
