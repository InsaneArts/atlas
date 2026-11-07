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

    /// The release-note task replayed over Activity.
    @MainActor public static let viewerActivityReplay: ViewerState = {
      var activity = openedActivity
      activity.replay = picturesReplay
      var state = viewerActivity
      state.pane = .activity(activity)
      return state
    }()
  }
#endif
