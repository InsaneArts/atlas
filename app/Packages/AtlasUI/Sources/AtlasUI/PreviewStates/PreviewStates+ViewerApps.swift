#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// One of the Apps samples' windows.
    ///
    /// - Parameters:
    ///   - id: The window's number.
    ///   - title: Its title.
    ///   - size: Its size.
    ///   - isFront: Whether it is the front window of the app in front.
    /// - Returns: The row.
    static func window(_ id: Int, _ title: String, _ size: String, isFront: Bool = false) -> ViewerWindowRow {
      ViewerWindowRow(id: id, title: title, size: size, showLabel: "Show \(title) on the screen", isFront: isFront)
    }

    /// A window's frame on the samples' 1440 by 900 display, as shares of it.
    ///
    /// - Parameters:
    ///   - x: Its left edge, in points.
    ///   - y: Its top edge, in points.
    ///   - width: Its width, in points.
    ///   - height: Its height, in points.
    /// - Returns: The shares.
    static func onDisplay(_ x: CGFloat, _ y: CGFloat, _ width: CGFloat, _ height: CGFloat) -> CGRect {
      CGRect(x: x / 1440, y: y / 900, width: width / 1440, height: height / 900)
    }

    /// A card's picture of where its windows sit on the samples' display, with no window's picture.
    ///
    /// - Parameter windows: The windows, front to back, as shares of the display.
    /// - Returns: The picture.
    static func sampleMap(_ windows: [CGRect]) -> ViewerAppPicture {
      ViewerAppPicture(map: ViewerScreenMap(aspect: 1.6, windows: windows))
    }

    /// The sample desktop's one window, 806 by 630 at 230, 108, as the Apps samples' front window: where it sits, and
    /// its picture cut from the sample desktop with 32 points of the screen around it.
    ///
    /// - Parameters:
    ///   - id: The window's number.
    ///   - behind: The app's other windows, as shares of the display.
    /// - Returns: The picture.
    @MainActor static func sampleFrontPicture(_ id: Int, behind: [CGRect] = []) -> ViewerAppPicture {
      let front = onDisplay(230, 108, 806, 630)
      return ViewerAppPicture(
        map: ViewerScreenMap(aspect: 1.6, windows: [front] + behind),
        window: SampleDesktop.picture?.cropped(to: onDisplay(198, 76, 870, 694), id: "sample-desktop.\(id)")
      )
    }

    /// The session icon and name an agent's card shows while every session's apps show.
    static let claudeOrigin = ViewerAppOrigin(icon: SessionSamples.claudeIcon, name: SessionSamples.claudeName)
    /// Codex's, likewise.
    static let codexOrigin = ViewerAppOrigin(icon: SessionSamples.codexIcon, name: SessionSamples.codexName)

    /// An app's icon in the samples, or its tile where there is none.
    ///
    /// - Parameters:
    ///   - id: Its bundle identifier.
    ///   - name: Its name.
    /// - Returns: The icon or the tile.
    @MainActor static func sampleIcon(_ id: String, _ name: String) -> AppIconFace {
      .of(SampleAppIcons.icons[id], name: name)
    }

    /// Opie, run from a copy and in front, its front window cut from the screen.
    ///
    /// - Parameter origin: The session named on its card; nil while only that session's apps show.
    /// - Returns: The card.
    @MainActor static func sampleOpie(origin: ViewerAppOrigin?) -> ViewerAppCard {
      ViewerAppCard(
        id: 501,
        name: "Opie",
        icon: sampleIcon(SampleAppIcons.opie, "Opie"),
        origin: origin,
        line: origin == nil ? "Opened 4 min ago" : "4 min ago",
        tags: [ViewerAppTag(words: "From a copy", tone: .attention), ViewerAppTag(words: "In front", tone: .accent)],
        isFront: true,
        picture: sampleFrontPicture(12, behind: [onDisplay(300, 160, 1080, 590)]),
        windows: [window(12, "Ring settings", "806 × 630", isFront: true), window(11, "Starter", "1080 × 590")]
      )
    }

    /// Probe, a build one of Claude Code's tasks installed, with no icon of its own, so it draws its tile; its window
    /// behind Opie's.
    ///
    /// - Parameter origin: The session named on its card; nil while only that session's apps show.
    /// - Returns: The card.
    @MainActor static func sampleProbe(origin: ViewerAppOrigin?) -> ViewerAppCard {
      ViewerAppCard(
        id: 506,
        name: "Probe",
        origin: origin,
        line: origin == nil ? "Opened 11 min ago" : "11 min ago",
        tags: [ViewerAppTag(words: "Installed by Atlas", tone: .attention)],
        picture: sampleMap([onDisplay(160, 220, 640, 480)]),
        windows: [window(61, "Untitled", "640 × 480")]
      )
    }

    /// The Apps sample's apps agents opened, in the order they opened.
    @MainActor static let sampleOpenedApps = [
      sampleProbe(origin: claudeOrigin),
      sampleOpie(origin: claudeOrigin),
      ViewerAppCard(
        id: 503,
        name: "Notes",
        icon: sampleIcon(SampleAppIcons.notes, "Notes"),
        origin: codexOrigin,
        line: "3 min ago",
        tags: [ViewerAppTag(words: "Hidden", tone: .quiet)],
        picture: sampleMap([onDisplay(520, 200, 900, 640)]),
        windows: [window(31, "QA checklist", "900 × 640")]
      ),
    ]

    /// The Apps sample's apps already open, in the order they launched.
    @MainActor static let sampleOpenApps = [
      ViewerAppCard(
        id: 505,
        name: "Finder",
        icon: sampleIcon(SampleAppIcons.finder, "Finder"),
        line: "Open since 09:02"
      ),
      ViewerAppCard(
        id: 502,
        name: "Safari",
        icon: sampleIcon(SampleAppIcons.safari, "Safari"),
        line: "Open since 09:12",
        picture: sampleMap([onDisplay(120, 80, 1200, 780)]),
        windows: [window(21, "Opie — Release notes", "1200 × 780")]
      ),
      ViewerAppCard(
        id: 504,
        name: "Terminal",
        icon: sampleIcon(SampleAppIcons.terminal, "Terminal"),
        line: "Open since 09:40",
        picture: sampleMap([onDisplay(680, 400, 720, 460)]),
        windows: [window(41, "atlas — zsh", "720 × 460")]
      ),
    ]

    /// The Apps sample's pane: the apps agents opened, then those already open.
    @MainActor static let sampleAppsPane = ViewerAppsState(
      title: "6 apps open",
      detail: "6 windows",
      groups: [
        ViewerAppGroup(id: "opened", title: "Opened by agents", cards: sampleOpenedApps),
        ViewerAppGroup(id: "already", title: "Already open", cards: sampleOpenApps),
      ]
    )

    /// Apps while the helper is idle, every session's apps showing, with the live screen in the corner.
    @MainActor public static let viewerApps: ViewerState = {
      var state = viewerIdle
      state.sidebar.library = viewerLibrary(.apps, counts: [.apps: 6])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .apps(sampleAppsPane)
      return state
    }()

    /// Apps while the picture is paused: the screen in the corner fades, and no window's picture is cut from it, so
    /// Opie's card shows where its windows sit.
    @MainActor public static let viewerAppsPaused: ViewerState = {
      var state = viewerPaused
      state.sidebar.library = viewerLibrary(.apps, counts: [.apps: 6])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0, isPaused: true) }
      var pane = sampleAppsPane
      pane.groups[0].cards[1].picture?.window = nil
      state.pane = .apps(pane)
      return state
    }()

    /// Claude Code chosen on Apps: only the apps it opened, with no heading, each saying when it opened them.
    @MainActor public static let viewerAppsOneSession: ViewerState = {
      var state = viewerActivityOneSession
      state.sidebar.library = viewerLibrary(.apps, counts: [.activity: 6, .screenshots: 0, .apps: 2])
      state.pane = .apps(
        ViewerAppsState(
          title: "2 apps open",
          detail: "3 windows",
          groups: [
            ViewerAppGroup(id: "session", title: nil, cards: [sampleProbe(origin: nil), sampleOpie(origin: nil)])
          ]
        )
      )
      return state
    }()

    /// Claude Code chosen on Apps before it opened any: the apps other sessions opened and those already open stay
    /// out, and the pane says so.
    @MainActor public static let viewerAppsOneSessionEmpty: ViewerState = {
      var state = viewerAppsOneSession
      state.sidebar.library = viewerLibrary(.apps, counts: [.activity: 6, .screenshots: 0, .apps: 0])
      state.pane = .apps(
        ViewerAppsState(
          title: "",
          detail: "",
          groups: [],
          empty: EmptyMessageState(
            headline: "This session hasn't opened any apps",
            message: "Apps it opens in the helper show up here. All sessions shows every app."
          )
        )
      )
      return state
    }()

    /// A copy Atlas made of a build only the helper can read, in front: its tile, since its own icon cannot be read,
    /// beside its front window cut from the screen.
    @MainActor public static let viewerAppsCopy: ViewerState = {
      var state = viewerApps
      state.sidebar.library = viewerLibrary(.apps, counts: [.apps: 2])
      state.pane = .apps(
        ViewerAppsState(
          title: "2 apps open",
          detail: "1 window",
          groups: [
            ViewerAppGroup(
              id: "opened",
              title: "Opened by agents",
              cards: [
                ViewerAppCard(
                  id: 601,
                  name: "Probe",
                  origin: claudeOrigin,
                  line: "just now",
                  tags: [
                    ViewerAppTag(words: "From a copy", tone: .attention),
                    ViewerAppTag(words: "In front", tone: .accent),
                  ],
                  isFront: true,
                  picture: sampleFrontPicture(71),
                  windows: [window(71, "Probe", "806 × 630", isFront: true)]
                )
              ]
            ),
            ViewerAppGroup(id: "already", title: "Already open", cards: [sampleOpenApps[0]]),
          ]
        )
      )
      return state
    }()

    /// Two hidden apps beside the app in front: each says it is hidden and shows where its windows sit.
    @MainActor public static let viewerAppsHidden: ViewerState = {
      var state = viewerApps
      state.sidebar.library = viewerLibrary(.apps, counts: [.apps: 3])
      state.pane = .apps(
        ViewerAppsState(
          title: "3 apps open",
          detail: "5 windows",
          groups: [
            ViewerAppGroup(
              id: "opened",
              title: "Opened by agents",
              cards: [sampleOpie(origin: claudeOrigin), sampleOpenedApps[2]]
            ),
            ViewerAppGroup(
              id: "already",
              title: "Already open",
              cards: [
                ViewerAppCard(
                  id: 507,
                  name: "TextEdit",
                  icon: sampleIcon(SampleAppIcons.textEdit, "TextEdit"),
                  line: "Open since 08:55",
                  tags: [ViewerAppTag(words: "Hidden", tone: .quiet)],
                  picture: sampleMap([onDisplay(80, 90, 620, 520), onDisplay(760, 260, 560, 480)]),
                  windows: [window(71, "a.txt", "620 × 520"), window(72, "Untitled", "560 × 480")]
                )
              ]
            ),
          ]
        )
      )
      return state
    }()

    /// Ten apps, one of them with five windows: its card lists two and puts the rest in "3 more windows", and every
    /// row of cards is as tall as its tallest.
    @MainActor public static let viewerAppsMany: ViewerState = {
      var state = viewerApps
      state.sidebar.library = viewerLibrary(.apps, counts: [.apps: 10])
      let safari = ViewerAppCard(
        id: 508,
        name: "Safari",
        icon: sampleIcon(SampleAppIcons.safari, "Safari"),
        origin: codexOrigin,
        line: "2 min ago",
        picture: sampleMap([
          onDisplay(120, 80, 1200, 780), onDisplay(160, 120, 1100, 720), onDisplay(200, 160, 1000, 680),
          onDisplay(240, 200, 900, 640), onDisplay(280, 240, 800, 600),
        ]),
        windows: [
          window(81, "Opie — Release notes", "1200 × 780"), window(82, "Ring settings guide", "1100 × 720"),
          window(83, "Pricing", "1000 × 680"), window(84, "Changelog", "900 × 640"), window(85, "Support", "800 × 600"),
        ]
      )
      let buildTool = ViewerAppCard(
        id: 509,
        name: "BuildTool",
        origin: claudeOrigin,
        line: "at 09:20",
        tags: [ViewerAppTag(words: "Installed by Atlas", tone: .attention)],
        picture: sampleMap([onDisplay(400, 300, 560, 400)]),
        windows: [window(91, "BuildTool", "560 × 400")]
      )
      let mail = ViewerAppCard(id: 510, name: "Mail", line: "Open since 08:40")
      let calendar = ViewerAppCard(
        id: 511,
        name: "Calendar",
        line: "Open since 08:41",
        picture: sampleMap([onDisplay(300, 140, 900, 600)]),
        windows: [window(92, "Calendar", "900 × 600")]
      )
      let textEdit = ViewerAppCard(
        id: 507,
        name: "TextEdit",
        icon: sampleIcon(SampleAppIcons.textEdit, "TextEdit"),
        line: "Open since 08:55",
        picture: sampleMap([onDisplay(80, 90, 620, 520)]),
        windows: [window(71, "a.txt", "620 × 520")]
      )
      state.pane = .apps(
        ViewerAppsState(
          title: "10 apps open",
          detail: "13 windows",
          groups: [
            ViewerAppGroup(id: "opened", title: "Opened by agents", cards: [buildTool] + sampleOpenedApps + [safari]),
            ViewerAppGroup(
              id: "already",
              title: "Already open",
              cards: [mail, calendar, textEdit, sampleOpenApps[0], sampleOpenApps[2]]
            ),
          ]
        )
      )
      return state
    }()
  }
#endif
