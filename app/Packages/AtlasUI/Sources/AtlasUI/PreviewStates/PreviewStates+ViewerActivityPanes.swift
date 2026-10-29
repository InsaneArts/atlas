#if DEBUG
  extension PreviewStates {
    /// The working sample's pane: Opie held open while its click runs, Notes open with its typing refused and done
    /// again, TextEdit and an older command folded.
    @MainActor static let workingActivity = ViewerActivityState(
      meta: "19 actions today",
      filters: activityFilters(.all, problems: 1, screenshots: 1),
      items: [
        .moment(
          ViewerActivityMoment(
            id: "session:|o1",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            time: "10:31",
            span: "6 actions · 10:31",
            age: "now",
            isOpen: true,
            canFold: false
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o6",
            kind: .pointer,
            mark: .live,
            words: ViewerStepWords("Clicking", target: "Show Center"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o5",
            kind: .scroll,
            words: ViewerStepWords("Scrolled"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o4",
            kind: .screenshot,
            words: ViewerStepWords("Took a screenshot"),
            time: "22 s",
            shot: sampleShot(shotNames[2])
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o3",
            kind: .typing,
            words: ViewerStepWords("Typed into", target: "Name"),
            time: "25 s"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o2",
            kind: .keys,
            words: ViewerStepWords("Pressed", keys: [["⌘", "S"]]),
            time: "27 s"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o1",
            kind: .window,
            words: ViewerStepWords("Opened Opie"),
            time: "30 s",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|n1",
            icon: sampleApp("com.apple.Notes", "Notes"),
            title: "Notes",
            time: "10:28",
            span: "3 actions · 10:28",
            age: "3 min",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "n3",
            kind: .typing,
            words: ViewerStepWords("Typed"),
            time: "3 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "n2",
            kind: .typing,
            mark: .notSent,
            words: ViewerStepWords("Didn't type"),
            time: "3 min",
            problem: ViewerActivityProblem(
              why: "Another app was in front.",
              next: "The agent tried again, and it worked."
            )
          )
        ),
        .step(
          ViewerActivityStep(
            id: "n1",
            kind: .window,
            words: ViewerStepWords("Opened Notes"),
            time: "3 min",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|e1",
            icon: sampleApp("com.apple.TextEdit", "TextEdit"),
            title: "TextEdit",
            time: "10:21",
            span: "9 actions · 10:21–10:24",
            age: "7 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|c1",
            icon: .symbol(.terminal),
            title: "Commands",
            time: "10:16",
            span: "1 action · 10:16",
            age: "15 min",
            isOpen: false
          )
        ),
      ]
    )

    /// The working sample's pane with TextEdit opened by the person, its newest eight steps then Show 1 more, and
    /// Notes folded by them.
    @MainActor static let openedActivity = ViewerActivityState(
      meta: "19 actions today",
      filters: activityFilters(.all, problems: 1, screenshots: 1),
      items: [
        .moment(
          ViewerActivityMoment(
            id: "session:|o1",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            time: "10:31",
            span: "6 actions · 10:31",
            age: "now",
            isOpen: true,
            canFold: false
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o6",
            kind: .pointer,
            mark: .live,
            words: ViewerStepWords("Clicking", target: "Show Center"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o5",
            kind: .scroll,
            words: ViewerStepWords("Scrolled"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o4",
            kind: .screenshot,
            words: ViewerStepWords("Took a screenshot"),
            time: "22 s",
            shot: sampleShot(shotNames[2])
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o3",
            kind: .typing,
            words: ViewerStepWords("Typed into", target: "Name"),
            time: "25 s"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o2",
            kind: .keys,
            words: ViewerStepWords("Pressed", keys: [["⌘", "S"]]),
            time: "27 s"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "o1",
            kind: .window,
            words: ViewerStepWords("Opened Opie"),
            time: "30 s",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|n1",
            icon: sampleApp("com.apple.Notes", "Notes"),
            title: "Notes",
            time: "10:28",
            span: "3 actions · 10:28",
            age: "3 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|e1",
            icon: sampleApp("com.apple.TextEdit", "TextEdit"),
            title: "TextEdit",
            time: "10:21",
            span: "9 actions · 10:21–10:24",
            age: "7 min",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e9",
            kind: .keys,
            words: ViewerStepWords("Pressed", keys: [["⌘", "S"]]),
            time: "7 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e8",
            kind: .typing,
            words: ViewerStepWords("Typed"),
            time: "7 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e7",
            kind: .pointer,
            words: ViewerStepWords("Clicked", target: "Bold"),
            time: "7 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e6",
            kind: .pointer,
            words: ViewerStepWords("Clicked", target: "Format"),
            time: "8 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e5",
            kind: .problem,
            mark: .failed,
            words: ViewerStepWords("Couldn't click Save"),
            time: "8 min",
            problem: ViewerActivityProblem(
              why: "No element is named Save.",
              next: "The agent tried again, and it worked."
            )
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e4",
            kind: .scroll,
            words: ViewerStepWords("Scrolled"),
            time: "9 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e3",
            kind: .typing,
            words: ViewerStepWords("Typed"),
            time: "9 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e2",
            kind: .pointer,
            words: ViewerStepWords("Clicked"),
            time: "9 min",
            isLast: true
          )
        ),
        .more(ViewerMoreSteps(moment: "session:|e1", count: 1)),
        .moment(
          ViewerActivityMoment(
            id: "session:|c1",
            icon: .symbol(.terminal),
            title: "Commands",
            time: "10:16",
            span: "1 action · 10:16",
            age: "15 min",
            isOpen: false
          )
        ),
      ]
    )

    /// The working sample's pane under Problems: TextEdit held open on the click that failed, and what came next.
    @MainActor static let problemsActivity = ViewerActivityState(
      meta: "19 actions today",
      filters: activityFilters(.problems, problems: 1, screenshots: 1),
      items: [
        .moment(
          ViewerActivityMoment(
            id: "session:|e1",
            icon: sampleApp("com.apple.TextEdit", "TextEdit"),
            title: "TextEdit",
            time: "10:21",
            span: "1 action · 10:21–10:24",
            age: "7 min",
            isOpen: true,
            canFold: false
          )
        ),
        .step(
          ViewerActivityStep(
            id: "e5",
            kind: .problem,
            mark: .failed,
            words: ViewerStepWords("Couldn't click Save"),
            time: "8 min",
            problem: ViewerActivityProblem(
              why: "No element is named Save.",
              next: "The agent tried again, and it worked."
            ),
            isLast: true
          )
        ),
      ]
    )

    /// A pane of the days before: yesterday's Safari open under its heading, the day before's BuildTool, a copy with
    /// its tile, folded.
    @MainActor static let daysActivity = ViewerActivityState(
      meta: "No actions today",
      filters: activityFilters(.all, problems: 0, screenshots: 1),
      items: [
        .day(ViewerActivityDay(id: "1792540800", title: "Yesterday")),
        .moment(
          ViewerActivityMoment(
            id: "session:|s1",
            icon: sampleApp("com.apple.Safari", "Safari"),
            title: "Safari",
            time: "23:24",
            span: "3 actions · 23:24–23:25",
            age: "23:25",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "s3",
            kind: .pointer,
            words: ViewerStepWords("Clicked", target: "Reload"),
            time: "23:25"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "s2",
            kind: .typing,
            words: ViewerStepWords("Typed into", target: "Address"),
            time: "23:24"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "s1",
            kind: .window,
            words: ViewerStepWords("Opened Safari"),
            time: "23:24",
            isLast: true
          )
        ),
        .day(ViewerActivityDay(id: "1792454400", title: "Di. 20. Okt.")),
        .moment(
          ViewerActivityMoment(
            id: "session:|b1",
            icon: sampleApp(nil, "BuildTool"),
            title: "BuildTool",
            time: "14:04",
            span: "3 actions · 14:04–14:05",
            age: "14:05",
            isOpen: false
          )
        ),
      ]
    )

    /// A morning of moving between apps every five minutes: the newest moment open, nine folded.
    @MainActor static let manyActivity = ViewerActivityState(
      meta: "20 actions today",
      filters: activityFilters(.all, problems: 0, screenshots: 0),
      items: [
        .moment(
          ViewerActivityMoment(
            id: "session:|m0a",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            time: "10:30",
            span: "2 actions · 10:30",
            age: "1 min",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "m0b",
            kind: .pointer,
            words: ViewerStepWords("Clicked"),
            time: "1 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "m0a",
            kind: .window,
            words: ViewerStepWords("Opened Opie"),
            time: "1 min",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m1a",
            icon: sampleApp("com.apple.Notes", "Notes"),
            title: "Notes",
            time: "10:25",
            span: "2 actions · 10:25",
            age: "6 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m2a",
            icon: sampleApp("com.apple.TextEdit", "TextEdit"),
            title: "TextEdit",
            time: "10:20",
            span: "2 actions · 10:20",
            age: "11 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m3a",
            icon: sampleApp("com.apple.Safari", "Safari"),
            title: "Safari",
            time: "10:15",
            span: "2 actions · 10:15",
            age: "16 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m4a",
            icon: sampleApp(nil, "BuildTool"),
            title: "BuildTool",
            time: "10:10",
            span: "2 actions · 10:10",
            age: "21 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m5a",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            time: "10:05",
            span: "2 actions · 10:05",
            age: "26 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m6a",
            icon: sampleApp("com.apple.Notes", "Notes"),
            title: "Notes",
            time: "10:00",
            span: "2 actions · 10:00",
            age: "31 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m7a",
            icon: sampleApp("com.apple.TextEdit", "TextEdit"),
            title: "TextEdit",
            time: "09:55",
            span: "2 actions · 09:55",
            age: "36 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m8a",
            icon: sampleApp("com.apple.Safari", "Safari"),
            title: "Safari",
            time: "09:50",
            span: "2 actions · 09:50",
            age: "41 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|m9a",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            time: "09:45",
            span: "2 actions · 09:45",
            age: "46 min",
            isOpen: false
          )
        ),
      ]
    )

    /// Every session's work: each moment named by its session, the person's own as You.
    @MainActor static let bySessionActivity = ViewerActivityState(
      meta: "14 actions today",
      filters: activityFilters(.all, problems: 0, screenshots: 1),
      items: [
        .moment(
          ViewerActivityMoment(
            id: "session:3f9a1c07b2e4|34",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            session: ViewerMomentSession(icon: SessionSamples.claudeIcon, name: SessionSamples.claudeName),
            time: "10:31",
            span: "5 actions · 10:31",
            age: "now",
            isOpen: true,
            canFold: false
          )
        ),
        .step(
          ViewerActivityStep(
            id: "38",
            kind: .pointer,
            mark: .live,
            words: ViewerStepWords("Clicking", target: "Show Center"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "37",
            kind: .scroll,
            words: ViewerStepWords("Scrolled"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "36",
            kind: .pointer,
            words: ViewerStepWords("Clicked", target: "Ring"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "35",
            kind: .screenshot,
            words: ViewerStepWords("Took a screenshot"),
            time: "22 s",
            shot: sampleShot(shotNames[2])
          )
        ),
        .step(
          ViewerActivityStep(
            id: "34",
            kind: .window,
            words: ViewerStepWords("Opened Opie"),
            time: "30 s",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:8be20c4d1a77|x1",
            icon: sampleApp("com.apple.Notes", "Notes"),
            title: "Notes",
            session: ViewerMomentSession(icon: SessionSamples.codexIcon, name: SessionSamples.codexName),
            time: "10:29",
            span: "2 actions · 10:29–10:30",
            age: "1 min",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "x2",
            kind: .typing,
            mark: .notSent,
            words: ViewerStepWords("Didn't type"),
            time: "1 min",
            problem: ViewerActivityProblem(why: "Another app was in front.")
          )
        ),
        .step(
          ViewerActivityStep(
            id: "x1",
            kind: .window,
            words: ViewerStepWords("Opened Notes"),
            time: "1 min",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "you|30",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            session: ViewerMomentSession(icon: nil, name: "You"),
            time: "10:29",
            span: "4 actions · 10:29",
            age: "2 min",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "33",
            kind: .control,
            words: ViewerStepWords("Gave back control"),
            time: "2 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "30",
            kind: .person,
            words: ViewerStepWords("Clicked", count: "×3"),
            time: "2 min",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:5d21e9a04c3f|u1",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            session: ViewerMomentSession(icon: SessionSamples.cursorIcon, name: SessionSamples.cursorName),
            time: "10:27",
            span: "1 action · 10:27",
            age: "4 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|t1",
            icon: .symbol(.terminal),
            title: "Commands",
            session: ViewerMomentSession(icon: SessionSamples.terminalIcon, name: SessionSamples.terminalName),
            time: "10:26",
            span: "1 action · 10:26",
            age: "5 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:3f9a1c07b2e4|27",
            icon: .symbol(.terminal),
            title: "Commands",
            session: ViewerMomentSession(icon: SessionSamples.claudeIcon, name: SessionSamples.claudeName),
            time: "10:25",
            span: "1 action · 10:25",
            age: "6 min",
            isOpen: false
          )
        ),
      ]
    )

    /// Every session's work while Claude Code's click may not have finished: its moment held open on the click,
    /// which offers Show Screen and Recover….
    @MainActor static let needsYouActivity = ViewerActivityState(
      meta: "14 actions today",
      filters: activityFilters(.all, problems: 1, screenshots: 1),
      items: [
        .moment(
          ViewerActivityMoment(
            id: "session:3f9a1c07b2e4|34",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            session: ViewerMomentSession(icon: SessionSamples.claudeIcon, name: SessionSamples.claudeName),
            time: "10:31",
            span: "5 actions · 10:31",
            age: "now",
            isOpen: true,
            canFold: false
          )
        ),
        .step(
          ViewerActivityStep(
            id: "c1",
            kind: .problem,
            mark: .failed,
            words: ViewerStepWords("Clicked Show Center"),
            time: "now",
            problem: ViewerActivityProblem(why: "May not have finished. Every session waits until you recover."),
            offersRecovery: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "37",
            kind: .scroll,
            words: ViewerStepWords("Scrolled"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "36",
            kind: .pointer,
            words: ViewerStepWords("Clicked", target: "Ring"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "35",
            kind: .screenshot,
            words: ViewerStepWords("Took a screenshot"),
            time: "22 s",
            shot: sampleShot(shotNames[2])
          )
        ),
        .step(
          ViewerActivityStep(
            id: "34",
            kind: .window,
            words: ViewerStepWords("Opened Opie"),
            time: "30 s",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:8be20c4d1a77|x1",
            icon: sampleApp("com.apple.Notes", "Notes"),
            title: "Notes",
            session: ViewerMomentSession(icon: SessionSamples.codexIcon, name: SessionSamples.codexName),
            time: "10:29",
            span: "2 actions · 10:29–10:30",
            age: "1 min",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "x2",
            kind: .typing,
            mark: .notSent,
            words: ViewerStepWords("Didn't type"),
            time: "1 min",
            problem: ViewerActivityProblem(why: "Another app was in front.")
          )
        ),
        .step(
          ViewerActivityStep(
            id: "x1",
            kind: .window,
            words: ViewerStepWords("Opened Notes"),
            time: "1 min",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "you|30",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            session: ViewerMomentSession(icon: nil, name: "You"),
            time: "10:29",
            span: "4 actions · 10:29",
            age: "2 min",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "33",
            kind: .control,
            words: ViewerStepWords("Gave back control"),
            time: "2 min"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "30",
            kind: .person,
            words: ViewerStepWords("Clicked", count: "×3"),
            time: "2 min",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:5d21e9a04c3f|u1",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            session: ViewerMomentSession(icon: SessionSamples.cursorIcon, name: SessionSamples.cursorName),
            time: "10:27",
            span: "1 action · 10:27",
            age: "4 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:|t1",
            icon: .symbol(.terminal),
            title: "Commands",
            session: ViewerMomentSession(icon: SessionSamples.terminalIcon, name: SessionSamples.terminalName),
            time: "10:26",
            span: "1 action · 10:26",
            age: "5 min",
            isOpen: false
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:3f9a1c07b2e4|27",
            icon: .symbol(.terminal),
            title: "Commands",
            session: ViewerMomentSession(icon: SessionSamples.claudeIcon, name: SessionSamples.claudeName),
            time: "10:25",
            span: "1 action · 10:25",
            age: "6 min",
            isOpen: false
          )
        ),
      ]
    )

    /// Claude Code's work alone: Opie held open while its click runs, an older command folded.
    @MainActor static let oneSessionActivity = ViewerActivityState(
      title: SessionSamples.claudeName,
      meta: "6 actions today",
      filters: activityFilters(.all, problems: 0, screenshots: 1),
      items: [
        .moment(
          ViewerActivityMoment(
            id: "session:3f9a1c07b2e4|34",
            icon: sampleApp("com.insanearts.opie", "Opie"),
            title: "Opie",
            time: "10:31",
            span: "5 actions · 10:31",
            age: "now",
            isOpen: true,
            canFold: false
          )
        ),
        .step(
          ViewerActivityStep(
            id: "38",
            kind: .pointer,
            mark: .live,
            words: ViewerStepWords("Clicking", target: "Show Center"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "37",
            kind: .scroll,
            words: ViewerStepWords("Scrolled"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "36",
            kind: .pointer,
            words: ViewerStepWords("Clicked", target: "Ring"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "35",
            kind: .screenshot,
            words: ViewerStepWords("Took a screenshot"),
            time: "22 s",
            shot: sampleShot(shotNames[2])
          )
        ),
        .step(
          ViewerActivityStep(
            id: "34",
            kind: .window,
            words: ViewerStepWords("Opened Opie"),
            time: "30 s",
            isLast: true
          )
        ),
        .moment(
          ViewerActivityMoment(
            id: "session:3f9a1c07b2e4|27",
            icon: .symbol(.terminal),
            title: "Commands",
            time: "10:25",
            span: "1 action · 10:25",
            age: "6 min",
            isOpen: false
          )
        ),
      ]
    )

    /// One agent's typing in Notes refused while another app was in front, in a quiet callout, and the retry that
    /// worked.
    @MainActor static let focusActivity = ViewerActivityState(
      meta: "3 actions today",
      filters: activityFilters(.all, problems: 0, screenshots: 0),
      items: [
        .moment(
          ViewerActivityMoment(
            id: "session:|f1",
            icon: sampleApp("com.apple.Notes", "Notes"),
            title: "Notes",
            time: "10:31",
            span: "3 actions · 10:31",
            age: "now",
            isOpen: true
          )
        ),
        .step(
          ViewerActivityStep(
            id: "f3",
            kind: .typing,
            words: ViewerStepWords("Typed"),
            time: "now"
          )
        ),
        .step(
          ViewerActivityStep(
            id: "f2",
            kind: .typing,
            mark: .notSent,
            words: ViewerStepWords("Didn't type"),
            time: "now",
            problem: ViewerActivityProblem(
              why: "Another app was in front.",
              next: "The agent tried again, and it worked."
            )
          )
        ),
        .step(
          ViewerActivityStep(
            id: "f1",
            kind: .window,
            words: ViewerStepWords("Opened Notes"),
            time: "12 s",
            isLast: true
          )
        ),
      ]
    )

    /// Claude Code chosen before it did anything: what this session does shows up here.
    @MainActor static let oneSessionEmptyActivity = ViewerActivityState(
      title: SessionSamples.claudeName,
      meta: "",
      filters: activityFilters(.all, problems: 0, screenshots: 0),
      items: [],
      empty: EmptyMessageState(
        headline: "Nothing has happened yet",
        message: "What this session does in the helper shows up here."
      ),
      showsFilters: false
    )
  }
#endif
