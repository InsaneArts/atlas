#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The pictures taken before the release-note task's steps in the sample chat app and the sign-in page, oldest
    /// first, then the chat app with the note posted.
    static let chatPictureNames = [
      "1792665041000000000-before.jpg",
      "1792665064000000000-before.jpg",
      "1792665067000000000-before.jpg",
      "1792665069000000000-before.jpg",
      "1792665070000000000-after.jpg",
      "1792664772000000000-before.jpg",
    ]

    /// Where the release-note task's actions went in the sample chat app and the sign-in page, as shares of the
    /// picture: the message box, #general in the side, the pointer on it, and the password field.
    static let chatPlaces = (
      box: SampleChatScene.frame(left: 0.24, top: 0.79, width: 0.74, height: 0.15),
      general: SampleChatScene.frame(left: 0.015, top: 0.265, width: 0.19, height: 0.055),
      generalPoint: CGPoint(x: 0.05 + 0.09 * 0.9, y: 0.06 + 0.292 * 0.88),
      messages: CGPoint(x: 0.05 + 0.55 * 0.9, y: 0.06 + 0.4 * 0.88),
      password: SampleChatScene.frame(left: 0.34, top: 0.56, width: 0.32, height: 0.07)
    )

    /// A picture of the sample chat app at a stage, its thumbnail read.
    ///
    /// - Parameters:
    ///   - index: Its place in ``chatPictureNames``.
    ///   - stage: Where the task stands in it.
    /// - Returns: The picture.
    @MainActor static func chatPicture(_ index: Int, _ stage: SampleChatStage) -> ViewerActivityShot {
      let name = chatPictureNames[index]
      return ViewerActivityShot(name: name, thumbnail: SampleDesktop.chat(name, stage: stage))
    }

    /// The release-note task's marks: the scroll down the channels, the click on #general, the typing into the
    /// message box, Return pressed in it, and a password typed into the sign-in page.
    @MainActor static let chatMarks = (
      scroll: ViewerActionMark(shape: .scroll(arrow: .down, label: "Down ×12"), spot: chatPlaces.messages),
      click: ViewerActionMark(shape: .click(clicks: 1), spot: chatPlaces.generalPoint, area: chatPlaces.general),
      typing: ViewerActionMark(
        shape: .typing(text: SampleChatScene.releaseNote, detail: "Typed · 70 characters"),
        area: chatPlaces.box
      ),
      keys: ViewerActionMark(shape: .keys([["⏎"]], name: "Return"), area: chatPlaces.box),
      password: ViewerActionMark(shape: .typing(text: "••••••••", detail: "A password"), area: chatPlaces.password)
    )

    /// What the release-note task's steps say, oldest first.
    static let chatWords = (
      scroll: ViewerStepWords("Scrolled down", count: "×12"),
      click: ViewerStepWords("Clicked", target: "# general"),
      typing: ViewerStepWords(
        "Typed",
        quote: "Release 0.1.1 is out: faster setup…",
        joiner: "into",
        target: "Message #general"
      ),
      keys: ViewerStepWords("Pressed", joiner: "in", target: "Message #general", keys: [["⏎"]], keyName: "Return"),
      password: ViewerStepWords("Typed a password into", target: "Password")
    )

    /// The Activity pane while Claude Code posts a release note in Slack, each step's small picture cropped around
    /// the element it went to; its password typed into a sign-in page in Safari before; and Codex's press of a
    /// button TextEdit has none of.
    @MainActor static let picturesActivity = ViewerActivityState(
      meta: "11 actions today",
      filters: activityFilters(.all, problems: 1),
      days: [
        ViewerActivityDay(
          id: "1792627200",
          title: "",
          moments: [
            ViewerActivityMoment(
              id: "session:|s1",
              icon: sampleApp(nil, "Slack"),
              title: "Slack",
              session: ViewerMomentSession(icon: SessionSamples.claudeIcon, name: SessionSamples.claudeName),
              time: "10:30",
              span: "4 actions · 10:30–10:31",
              age: "now",
              isOpen: true,
              canReplay: true,
              steps: [
                ViewerActivityStep(
                  id: "s4",
                  kind: .keys,
                  words: chatWords.keys,
                  time: "now",
                  picture: ViewerStepPicture(shot: chatPicture(3, .typed), mark: chatMarks.keys)
                ),
                ViewerActivityStep(
                  id: "s3",
                  kind: .typing,
                  words: chatWords.typing,
                  time: "2 s",
                  picture: ViewerStepPicture(shot: chatPicture(2, .empty), mark: chatMarks.typing)
                ),
                ViewerActivityStep(
                  id: "s2",
                  kind: .pointer,
                  words: chatWords.click,
                  time: "5 s",
                  picture: ViewerStepPicture(shot: chatPicture(1, .start), mark: chatMarks.click)
                ),
                ViewerActivityStep(
                  id: "s1",
                  kind: .scroll,
                  words: chatWords.scroll,
                  time: "28 s",
                  picture: ViewerStepPicture(shot: chatPicture(0, .start), mark: chatMarks.scroll, repeats: 12),
                  isLast: true
                ),
              ]
            ),
            ViewerActivityMoment(
              id: "session:|w1",
              icon: sampleApp("com.apple.Safari", "Safari"),
              title: "Safari",
              session: ViewerMomentSession(icon: SessionSamples.claudeIcon, name: SessionSamples.claudeName),
              time: "10:26",
              span: "1 action · 10:26",
              age: "5 min",
              isOpen: true,
              steps: [
                ViewerActivityStep(
                  id: "w1",
                  kind: .typing,
                  words: chatWords.password,
                  time: "5 min",
                  picture: ViewerStepPicture(
                    shot: ViewerActivityShot(
                      name: chatPictureNames[5],
                      thumbnail: SampleDesktop.signIn(chatPictureNames[5])
                    ),
                    mark: chatMarks.password
                  ),
                  notKept: true,
                  isLast: true
                )
              ]
            ),
            ViewerActivityMoment(
              id: "session:|t1",
              icon: sampleApp("com.apple.TextEdit", "TextEdit"),
              title: "TextEdit",
              session: ViewerMomentSession(icon: SessionSamples.codexIcon, name: SessionSamples.codexName),
              time: "10:24",
              span: "1 action · 10:24",
              age: "7 min",
              isOpen: true,
              steps: [
                ViewerActivityStep(
                  id: "t1",
                  kind: .problem,
                  mark: .failed,
                  words: ViewerStepWords("Couldn't press", target: "Save"),
                  time: "7 min",
                  picture: ViewerStepPicture(
                    shot: samplePicture(beforeNames[1]),
                    mark: ViewerActionMark(
                      shape: .press,
                      area: CGRect(x: 0.16, y: 0.12, width: 0.56, height: 0.7),
                      tone: .failed,
                      note: "No “Save” in this window"
                    )
                  ),
                  problem: ViewerActivityProblem(
                    why: "No button is named Save in this window.",
                    next: "The agent pressed ⌘S next, and it worked."
                  ),
                  isLast: true
                )
              ]
            ),
          ]
        )
      ]
    )

    /// The release-note task replayed across two apps, on its typing: the sign-in in Safari, then in Slack the
    /// scroll, the click on #general, the typing, Return, and the screen with the note posted.
    @MainActor static let picturesReplay = ViewerReplayState(
      title: "Post the 0.1.1 release note",
      detail: "\(SessionSamples.claudeName) · 5 steps · 10:26:02 to 10:31:10",
      icon: SessionSamples.claudeIcon,
      frames: [
        ViewerReplayFrame(
          id: "w1",
          picture: ViewerActivityShot(name: chatPictureNames[5], thumbnail: SampleDesktop.signIn(chatPictureNames[5])),
          mark: chatMarks.password,
          words: chatWords.password,
          detail: "Safari · 10:26:02",
          kind: .typing,
          label: "1",
          app: "Safari"
        ),
        ViewerReplayFrame(
          id: "s1",
          picture: chatPicture(0, .start),
          mark: chatMarks.scroll,
          words: chatWords.scroll,
          detail: "Slack · 10:30:41 to 10:30:52 · 12 in one step",
          kind: .scroll,
          repeats: 12,
          label: "2",
          app: "Slack"
        ),
        ViewerReplayFrame(
          id: "s2",
          picture: chatPicture(1, .start),
          mark: chatMarks.click,
          words: chatWords.click,
          detail: "Slack · 10:31:04",
          kind: .pointer,
          label: "3",
          app: "Slack"
        ),
        ViewerReplayFrame(
          id: "s3",
          picture: chatPicture(2, .empty),
          mark: chatMarks.typing,
          words: chatWords.typing,
          detail: "Slack · 10:31:07",
          kind: .typing,
          label: "4",
          app: "Slack"
        ),
        ViewerReplayFrame(
          id: "s4",
          picture: chatPicture(3, .typed),
          mark: chatMarks.keys,
          words: chatWords.keys,
          detail: "Slack · 10:31:09",
          kind: .keys,
          label: "5",
          app: "Slack"
        ),
        ViewerReplayFrame(
          id: "after",
          picture: chatPicture(4, .posted),
          words: ViewerStepWords("The screen after the last step"),
          detail: "Slack · 10:31:10",
          label: "After",
          app: "Slack",
          isAfter: true
        ),
      ],
      index: 3
    )

    /// Return pressed in the message box shown large, compared on a slide with what changed shown: the note left the
    /// box and stood in the channel.
    @MainActor static let picturesLarge = ViewerActionLarge(
      id: "s4",
      words: chatWords.keys,
      context: "\(SessionSamples.claudeName) · Post the 0.1.1 release note · Slack · step 4 of 4",
      icon: SessionSamples.claudeIcon,
      before: chatPicture(3, .typed).thumbnail,
      after: chatPicture(4, .posted).thumbnail,
      hasAfter: true,
      aspect: CGSize(width: 1440, height: 900),
      mark: chatMarks.keys,
      beforeLabel: "Before · 10:31:09.21",
      afterLabel: "After · 10:31:10.02",
      details: [
        ViewerActionDetail(title: "What", value: "Pressed Return in Message #general"),
        ViewerActionDetail(title: "Where", value: "Slack › general · InsaneArts › Message #general"),
        ViewerActionDetail(
          title: "Agent and task",
          value: "\(SessionSamples.claudeName)\nPost the 0.1.1 release note"
        ),
        ViewerActionDetail(title: "Outcome", value: "Worked", tone: .worked),
        ViewerActionDetail(title: "Before", value: "10:31:09.21, just before the action"),
        ViewerActionDetail(title: "After", value: "10:31:10.02, a second later"),
      ],
      showsChanges: true,
      changes: [
        CGRect(x: 0.27, y: 0.42, width: 0.62, height: 0.13),
        CGRect(x: 0.27, y: 0.76, width: 0.66, height: 0.12),
      ],
      canGoBack: true,
      canGoForward: false
    )

    /// Activity with the release-note task's pictures.
    @MainActor public static let viewerActivityPictures: ViewerState = {
      var state = viewerActivity
      state.pane = .activity(picturesActivity)
      return state
    }()

    /// The release-note task replayed over Activity.
    @MainActor public static let viewerActivityReplay: ViewerState = {
      var activity = picturesActivity
      activity.replay = picturesReplay
      var state = viewerActivity
      state.pane = .activity(activity)
      return state
    }()

    /// Return pressed in the release-note task shown large over Activity, on a slide with what changed shown.
    @MainActor public static let viewerActionLarge: ViewerState = {
      var activity = picturesActivity
      activity.large = picturesLarge
      var state = viewerActivity
      state.pane = .activity(activity)
      return state
    }()
  }
#endif
