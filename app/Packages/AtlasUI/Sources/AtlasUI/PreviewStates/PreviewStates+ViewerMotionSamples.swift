#if DEBUG
  extension PreviewStates {
    /// Activity a moment after the agent moved on to Safari: Safari's run at the top of the live session, its first
    /// step running, and TextEdit's run done.
    @MainActor static let viewerActivityNewRun: ViewerState = {
      var state = viewerActivity
      var pane = workingActivity
      pane.days[0].sessions[0].runs[0].isRunning = false
      pane.days[0].sessions[0].runs[0].time = "01:51–01:58"
      pane.days[0].sessions[0].runs.insert(
        ViewerOutlineRun(
          id: "run.safari",
          icon: sampleApp(SampleAppIcons.safari, "Safari"),
          name: "Safari",
          summary: "Opening Safari · 1 action",
          time: "now",
          isRunning: true
        ),
        at: 0
      )
      state.pane = .activity(pane)
      return state
    }()

    /// Apps with Safari in front: Opie's card lowered in place, showing where its windows sit, and Safari's raised.
    @MainActor static let viewerAppsSafariFront: ViewerState = {
      var state = viewerApps
      state.pane = .apps(
        appsPane { card in
          switch card.id {
          case 501:
            card.isFront = false
            card.tags.removeAll { $0.tone == .accent }
            card.picture?.window = nil
            card.windows = card.windows.map { window in
              var behind = window
              behind.isFront = false
              return behind
            }
          case 502:
            card.isFront = true
            card.tags = [ViewerAppTag(words: "In front", tone: .accent)]
            card.windows = card.windows.map { window in
              var front = window
              front.isFront = true
              return front
            }
          default:
            break
          }
        }
      )
      return state
    }()

    /// Apps before Notes opened: five apps, and the agents' group one card shorter.
    @MainActor static let viewerAppsBeforeNotes: ViewerState = {
      var state = viewerApps
      state.sidebar.library = viewerLibrary(.apps, counts: [.apps: 5])
      var pane = sampleAppsPane
      pane.title = "5 apps open"
      pane.detail = "5 windows"
      pane.groups[0].cards.removeAll { $0.id == notesCard }
      state.pane = .apps(pane)
      return state
    }()

    /// Apps before the helper learned an agent opened Notes: its card at the end of Already open, saying since when it
    /// has been open, so the sample, where it is the agents', moves it between the groups.
    @MainActor static let viewerAppsNotesAlreadyOpen: ViewerState = {
      var state = viewerApps
      var pane = sampleAppsPane
      if let notes = pane.groups[0].cards.first(where: { $0.id == notesCard }) {
        var open = notes
        open.origin = nil
        open.line = "Open since 10:28"
        pane.groups[0].cards.removeAll { $0.id == notesCard }
        pane.groups[1].cards.append(open)
      }
      state.pane = .apps(pane)
      return state
    }()

    /// 503: Notes' card in the Apps sample.
    private static let notesCard = 503

    /// The Apps sample's pane with each card changed by `change`.
    ///
    /// - Parameter change: Changes a card.
    /// - Returns: The pane.
    @MainActor private static func appsPane(_ change: (inout ViewerAppCard) -> Void) -> ViewerAppsState {
      var pane = sampleAppsPane
      for group in pane.groups.indices {
        for card in pane.groups[group].cards.indices {
          change(&pane.groups[group].cards[card])
        }
      }
      return pane
    }
  }
#endif
