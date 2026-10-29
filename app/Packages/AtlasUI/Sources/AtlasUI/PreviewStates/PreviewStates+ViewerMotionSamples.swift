#if DEBUG
  extension PreviewStates {
    /// Activity with Notes folded by the person, its three actions on one row under Opie.
    @MainActor static let viewerActivityNotesFolded = viewer(
      viewerActivity,
      applying: .toggleMoment("session:|n1", opens: false)
    )

    /// Activity a moment before the click in Opie began: scrolling is the newest step, still running, and the day
    /// counts one action fewer, so the click arriving over it is the working sample.
    @MainActor static let viewerActivityBeforeStep: ViewerState = {
      var state = viewerActivity
      var pane = workingActivity
      pane.meta = "18 actions today"
      pane.items = pane.items.compactMap { item in
        switch item {
        case .step(let step) where step.id == "o6":
          return nil
        case .step(var step) where step.id == "o5":
          step.mark = .live
          step.words = ViewerStepWords("Scrolling")
          return .step(step)
        case .moment(var moment) where moment.id == "session:|o1":
          moment.span = "5 actions · 10:31"
          return .moment(moment)
        default:
          return item
        }
      }
      state.pane = .activity(pane)
      return state
    }()

    /// Activity with TextEdit's "Show 1 more" pressed: its oldest step, its opening, under the eight it listed.
    @MainActor static let viewerActivityEveryStep: ViewerState = {
      var state = viewerActivityOpened
      guard case .activity(var pane) = state.pane else { return state }
      pane.items = pane.items.flatMap { item -> [ViewerActivityItem] in
        switch item {
        case .step(var step) where step.id == "e2":
          step.isLast = false
          return [.step(step)]
        case .more(let more) where more.moment == "session:|e1":
          let opened = ViewerActivityStep(
            id: "e1",
            kind: .window,
            words: ViewerStepWords("Opened TextEdit"),
            time: "10 min",
            isLast: true
          )
          return [.step(opened)]
        default:
          return [item]
        }
      }
      state.pane = .activity(pane)
      return state
    }()

    /// Activity a moment after the agent moved on to Safari: Safari's moment at the top with its first step running,
    /// and Opie's click done, its moment now one the person can fold.
    @MainActor static let viewerActivityNewMoment: ViewerState = {
      var state = viewerActivity
      var pane = workingActivity
      pane.meta = "20 actions today"
      let safari: [ViewerActivityItem] = [
        .moment(
          ViewerActivityMoment(
            id: "session:|s1",
            icon: sampleApp(SampleAppIcons.safari, "Safari"),
            title: "Safari",
            time: "10:32",
            span: "1 action · 10:32",
            age: "now",
            isOpen: true,
            canFold: false
          )
        ),
        .step(
          ViewerActivityStep(
            id: "s1",
            kind: .window,
            mark: .live,
            words: ViewerStepWords("Opening Safari"),
            time: "now",
            isLast: true
          )
        ),
      ]
      pane.items =
        safari
        + pane.items.map { item in
          switch item {
          case .step(var step) where step.id == "o6":
            step.mark = .quiet
            step.words = ViewerStepWords("Clicked", target: "Show Center")
            return .step(step)
          case .moment(var moment) where moment.id == "session:|o1":
            moment.canFold = true
            return .moment(moment)
          default:
            return item
          }
        }
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
