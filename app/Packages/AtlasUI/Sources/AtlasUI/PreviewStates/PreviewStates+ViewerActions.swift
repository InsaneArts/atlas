#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The Activity sample's typing in Opie shown large: the screen before with its ring and bubble, the screen after,
    /// compared on a slide, with what Atlas knows beside them.
    @MainActor public static let actionLarge = ViewerActionLarge(
      id: "o3",
      words: ViewerStepWords("Typed", quote: sampleTyped, joiner: "into", target: "Name"),
      context: "Opie · step 2 of 2",
      before: shotPicture(beforeNames[0]),
      after: shotPicture(shotNames[2]),
      hasAfter: true,
      aspect: CGSize(width: 2560, height: 1440),
      mark: sampleTypingPicture.mark,
      beforeLabel: "Before · 10:31:17.00",
      afterLabel: "After · 10:31:20.00",
      details: [
        ViewerActionDetail(title: "What", value: "Typed “Ring settings”"),
        ViewerActionDetail(title: "Where", value: "Opie › Name"),
        ViewerActionDetail(title: "Outcome", value: "Worked", tone: .worked),
        ViewerActionDetail(title: "Before", value: "10:31:17.00, just before the action"),
        ViewerActionDetail(title: "After", value: "10:31:20.00, the next step's picture before it"),
      ],
      changes: [CGRect(x: 0.25, y: 0.3125, width: 0.1875, height: 0.125)],
      canGoBack: true,
      canGoForward: false
    )

    /// A click that failed shown large, with no picture after it, so the ways to compare leave its bar.
    @MainActor public static let actionLargeFailed = ViewerActionLarge(
      id: "e7",
      words: ViewerStepWords("Couldn't click", target: "Save"),
      context: "TextEdit · step 7 of 9",
      before: shotPicture(beforeNames[1]),
      hasAfter: false,
      aspect: CGSize(width: 2560, height: 1440),
      mark: ViewerActionMark(
        shape: .click(clicks: 1),
        spot: CGPoint(x: 0.62, y: 0.2),
        tone: .failed,
        note: "No “Save” in this window"
      ),
      beforeLabel: "Before · 10:24:02.40",
      details: [
        ViewerActionDetail(title: "What", value: "Clicked Save"),
        ViewerActionDetail(title: "Where", value: "TextEdit › Save"),
        ViewerActionDetail(title: "Outcome", value: "Failed: No “Save” in this window", tone: .failed),
        ViewerActionDetail(title: "Before", value: "10:24:02.40, just before the action"),
      ],
      canGoBack: true,
      canGoForward: true
    )

    /// The Activity sample's Opie task replayed: its ⌘S, its typing, then the screen after.
    @MainActor public static let replay = ViewerReplayState(
      title: "Opie · 10:31",
      detail: "2 steps · 10:31:15 to 10:31:20",
      frames: [
        ViewerReplayFrame(
          id: "o2",
          picture: samplePicture(beforeNames[1]),
          mark: sampleKeysPicture.mark,
          words: ViewerStepWords("Pressed", keys: [["⌘", "S"]]),
          detail: "Opie · 10:31:15",
          kind: .keys,
          label: "1"
        ),
        ViewerReplayFrame(
          id: "o3",
          picture: samplePicture(beforeNames[0]),
          mark: sampleTypingPicture.mark,
          words: ViewerStepWords("Typed", quote: sampleTyped, joiner: "into", target: "Name"),
          detail: "Opie · 10:31:17",
          kind: .typing,
          label: "2"
        ),
        ViewerReplayFrame(
          id: "after",
          picture: sampleShot(shotNames[2]),
          words: ViewerStepWords("The screen after the last step"),
          detail: "Opie · 10:31:20",
          label: "After",
          isAfter: true
        ),
      ]
    )

    /// One mark of each kind and tone over the sample desktop, as the marks preview lays them out.
    @MainActor static let sampleMarks: [(name: String, mark: ViewerActionMark)] = [
      ("Click", ViewerActionMark(shape: .click(clicks: 1), spot: CGPoint(x: 0.3, y: 0.4))),
      ("Double click", ViewerActionMark(shape: .click(clicks: 2), spot: CGPoint(x: 0.5, y: 0.5))),
      (
        "Typing",
        ViewerActionMark(
          shape: .typing(text: "Release 0.1.1 is out: faster setup", detail: "Typed · 34 characters"),
          spot: CGPoint(x: 0.5, y: 0.7)
        )
      ),
      (
        "Password",
        ViewerActionMark(shape: .typing(text: "••••••••", detail: "A password"), spot: CGPoint(x: 0.5, y: 0.6))
      ),
      ("Keys", ViewerActionMark(shape: .keys([["⌘", "S"]]), spot: CGPoint(x: 0.4, y: 0.5))),
      ("Keys, no spot", ViewerActionMark(shape: .keys([["⏎"]]))),
      ("Scroll", ViewerActionMark(shape: .scroll(arrow: .down, label: "Down ×12"), spot: CGPoint(x: 0.5, y: 0.45))),
      ("Drag", ViewerActionMark(shape: .drag(to: CGPoint(x: 0.75, y: 0.3)), spot: CGPoint(x: 0.25, y: 0.6))),
      ("Press", ViewerActionMark(shape: .press, spot: CGPoint(x: 0.45, y: 0.35))),
      ("Press by name", ViewerActionMark(shape: .press)),
      (
        "Failed",
        ViewerActionMark(
          shape: .click(clicks: 1),
          spot: CGPoint(x: 0.4, y: 0.3),
          tone: .failed,
          note: "No “Save” in this window"
        )
      ),
      (
        "Not sure it finished",
        ViewerActionMark(shape: .keys([["⏎"]]), spot: CGPoint(x: 0.5, y: 0.6), tone: .uncertain)
      ),
      (
        "Nothing sent",
        ViewerActionMark(
          shape: .typing(text: "Hello", detail: "Typed · 5 characters"),
          spot: CGPoint(x: 0.5, y: 0.55),
          tone: .notSent,
          note: "Another window was in front"
        )
      ),
    ]

    /// Steps' small pictures: a click, a scroll folded twelve times, a password, a failed click and one that sent
    /// nothing.
    @MainActor static let sampleStepPictures: [ViewerStepPicture] = [
      ViewerStepPicture(shot: samplePicture(beforeNames[0]), mark: sampleMarks[0].mark),
      ViewerStepPicture(shot: samplePicture(beforeNames[0]), mark: sampleMarks[6].mark, repeats: 12),
      ViewerStepPicture(shot: samplePicture(beforeNames[1]), mark: sampleMarks[3].mark),
      ViewerStepPicture(shot: samplePicture(beforeNames[1]), mark: sampleMarks[10].mark),
      ViewerStepPicture(shot: samplePicture(beforeNames[1]), mark: sampleMarks[12].mark),
    ]

    /// The Activity sample after one of its large view's or replay's events, roughly as the app answers them.
    ///
    /// - Parameters:
    ///   - activity: The pane shown.
    ///   - event: What was done.
    /// - Returns: The pane.
    @MainActor static func actionSample(_ activity: ViewerActivityState, applying event: ViewerEvent)
      -> ViewerActivityState
    {
      var next = activity
      let index = activity.replay?.index ?? 0
      switch event {
      case .openAction(let id): next.large = id == "o2" ? actionLargeFailed : actionLarge
      case .closeAction: next.large = nil
      case .previousAction: next.large = next.large.map { $0.id == "o3" ? actionLargeFailed : actionLarge }
      case .nextAction: next.large = next.large.map { $0.id == "o3" ? $0 : actionLarge }
      case .compareAction(let mode): next.large?.compare = mode
      case .toggleChanges: next.large?.showsChanges.toggle()
      case .openReplay: next.replay = replay
      case .closeReplay: next.replay = nil
      case .replayFrame(let picked): next.replay?.index = picked
      case .previousFrame: next.replay?.index = max(0, index - 1)
      case .nextFrame: next.replay?.index = min(replay.frames.count - 1, index + 1)
      case .togglePlay: next.replay?.isPlaying.toggle()
      default: break
      }
      return next
    }
  }
#endif
