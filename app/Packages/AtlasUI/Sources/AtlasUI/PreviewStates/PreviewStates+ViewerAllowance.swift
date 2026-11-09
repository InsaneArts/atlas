#if DEBUG
  extension PreviewStates {
    /// The person allowed the rest of Claude Code's task on github.com.
    static let allowedForTask = AllowanceBadge(words: "You allowed for this task", tone: .accent)
    /// The person allowed one action on github.com.
    static let allowedOnce = AllowanceBadge(words: "You allowed once", tone: .accent)
    /// An account the person trusts.
    static let trustedAccount = AllowanceBadge(words: "Trusted", tone: .quiet)

    /// Claude Code's run on github.com in Chrome, opened and titled by the site: a click that needed no allowing, a comment typed and sent
    /// once the person allowed it once, and the merge they allowed for the task.
    @MainActor static let githubRunOpen = ViewerOutlineRun(
      id: "run.github",
      icon: sampleApp(SampleAppIcons.safari, "Google Chrome"),
      name: "github.com",
      summary: "Clicked and typed · 4 actions",
      time: "02:10–02:12",
      isOpen: true,
      canReplay: true,
      steps: [
        ViewerActivityStep(
          id: "step.github.files",
          kind: .pointer,
          words: ViewerStepWords("Clicked", target: "Files changed"),
          time: "02:10",
          hasPicture: true
        ),
        ViewerActivityStep(
          id: "step.github.type",
          kind: .typing,
          words: ViewerStepWords("Typed", quote: "Looks good to me", joiner: "into", target: "Comment"),
          hasPicture: true,
          allowance: allowedForTask
        ),
        ViewerActivityStep(
          id: "step.github.comment",
          kind: .pointer,
          words: ViewerStepWords("Clicked", target: "Comment"),
          time: "02:11",
          hasPicture: true,
          allowance: allowedOnce
        ),
        ViewerActivityStep(
          id: "step.github.merge",
          kind: .pointer,
          words: ViewerStepWords("Clicked", target: "Merge pull request"),
          time: "02:12",
          hasPicture: true,
          allowance: allowedForTask
        ),
      ],
      account: "github.com"
    )

    /// Claude Code's run in a Trusted Slack, folded: its row says Trusted.
    @MainActor static let slackRunTrusted = ViewerOutlineRun(
      id: "run.slack",
      icon: sampleApp(SampleAppIcons.notes, "Slack"),
      name: "Slack",
      summary: "Typed and pressed keys · 6 actions",
      time: "02:08–02:09",
      account: "Slack",
      allowance: trustedAccount
    )

    /// Claude Code's run in Chrome across sites, opened: each step in a listed account names it beside its words,
    /// a search outside any names none, and the run, in no one account, keeps the browser's name and no badge.
    @MainActor static let mixedRunOpen = ViewerOutlineRun(
      id: "run.chrome.sites",
      icon: sampleApp(SampleAppIcons.safari, "Google Chrome"),
      name: "Google Chrome",
      summary: "Clicked and typed · 4 actions",
      time: "02:14–02:16",
      isOpen: true,
      canReplay: true,
      steps: [
        ViewerActivityStep(
          id: "step.sites.search",
          kind: .pointer,
          words: ViewerStepWords("Clicked", target: "Search"),
          time: "02:14",
          hasPicture: true
        ),
        ViewerActivityStep(
          id: "step.sites.merge",
          kind: .pointer,
          words: ViewerStepWords("Clicked", target: "Merge pull request"),
          hasPicture: true,
          account: "github.com",
          allowance: trustedAccount
        ),
        ViewerActivityStep(
          id: "step.sites.type",
          kind: .typing,
          words: ViewerStepWords("Typed", quote: "Ship it after the release notes", joiner: "into", target: "Comment"),
          time: "02:15",
          hasPicture: true,
          account: "staging.acme.dev",
          allowance: allowedOnce
        ),
        ViewerActivityStep(
          id: "step.sites.pulls",
          kind: .pointer,
          words: ViewerStepWords("Clicked", target: "Pull requests"),
          time: "02:16",
          hasPicture: true,
          account: "github.com"
        ),
      ]
    )

    /// Activity with Claude Code's run across sites opened, each step naming the account it acted in.
    @MainActor public static let viewerActivityAllowedMixed: ViewerState = {
      var state = viewerActivity
      var session = claudeSession
      session.runs = [mixedRunOpen, githubRunOpen] + session.runs
      var activity = workingActivity
      activity.days[0].sessions = [session]
      state.pane = .activity(activity)
      return state
    }()

    /// Activity with Claude Code's run on github.com opened, each step saying what let it act there, and its Trusted
    /// Slack run folded under it.
    @MainActor public static let viewerActivityAllowed: ViewerState = {
      var state = viewerActivity
      var session = claudeSession
      session.runs = [githubRunOpen, slackRunTrusted] + session.runs
      var activity = workingActivity
      activity.days[0].sessions = [session]
      state.pane = .activity(activity)
      return state
    }()
  }
#endif
