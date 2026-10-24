#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The Screenshots sample's file names, newest first: 10:31:45, 10:31:44, 10:31:20, 10:31:14, 10:12:03 and
    /// 09:58:41 on 22 October 2026, in UTC.
    public static let shotNames = [
      "1792665105000000000-capture.png",
      "1792665104000000000-capture.png",
      "1792665080000000000-capture.png",
      "1792665074000000000-capture.png",
      "1792663923000000000-capture.png",
      "1792663121000000000-capture.png",
    ]

    /// A screenshot's thumbnail in the samples: the sample desktop, known by the screenshot's name.
    ///
    /// - Parameter name: The screenshot's file name.
    /// - Returns: The thumbnail, or nil should the sample desktop not draw.
    @MainActor static func shotPicture(_ name: String) -> DesktopPicture? {
      SampleDesktop.picture
        .map { DesktopPicture(id: name, image: $0.image) }
    }

    /// One tile of the Screenshots sample.
    ///
    /// - Parameters:
    ///   - index: Its place in ``shotNames``.
    ///   - time: When it was taken.
    ///   - isSelected: Whether it is selected.
    /// - Returns: The tile.
    @MainActor static func shotTile(_ index: Int, _ time: String, isSelected: Bool = false) -> ViewerShotTile {
      ViewerShotTile(
        id: shotNames[index],
        time: time,
        thumbnail: shotPicture(shotNames[index]),
        isSelected: isSelected,
        label: "Screenshot taken at \(time)"
      )
    }

    /// The Screenshots tab an hour on, 10:31:44 selected (MacShots).
    @MainActor public static let viewerShots: ViewerState = {
      var state = viewerIdle
      state.top.status = StatusSummary(
        label: "Idle",
        tone: .quiet,
        mark: .dot,
        line: "Last: took a screenshot · 2 min ago"
      )
      state.top.tabs = viewerTabs(.screenshots)
      state.pane = .screenshots(
        ViewerShotsState(
          meta: "6 today · Space opens one, drag one out to use it",
          query: "",
          days: [
            ViewerShotDay(
              id: "hour",
              title: "Today",
              shots: [
                shotTile(0, "10:31:45"), shotTile(1, "10:31:44", isSelected: true), shotTile(2, "10:31:20"),
                shotTile(3, "10:31:14"),
              ]
            ),
            ViewerShotDay(
              id: "morning",
              title: "This morning",
              shots: [shotTile(4, "10:12:03"), shotTile(5, "09:58:41")]
            ),
          ]
        )
      )
      return state
    }()

    /// 10:31:44 shown large over the whole window (MacShotLarge).
    @MainActor public static let viewerShotLarge: ViewerState = {
      var state = viewerShots
      guard case .screenshots(var shots) = state.pane, let picture = SampleDesktop.picture else { return state }
      shots.large = ViewerLargeShot(
        id: shotNames[1],
        picture: DesktopPicture(id: "\(shotNames[1]) full", image: picture.image),
        aspect: CGSize(width: picture.image.width, height: picture.image.height),
        caption: "10:31:44 · 2560 × 1440",
        canGoBack: true,
        canGoForward: true,
        canShowInFinder: true
      )
      state.pane = .screenshots(shots)
      return state
    }()

    /// The Screenshots tab before any agent took one.
    @MainActor public static let viewerShotsEmpty: ViewerState = {
      var state = viewerShots
      state.pane = .screenshots(
        ViewerShotsState(
          meta: "None today",
          query: "",
          days: [],
          empty: EmptyMessageState(
            headline: "No screenshots yet",
            message: "Screenshots agents take in the helper show up here."
          )
        )
      )
      return state
    }()
  }
#endif
