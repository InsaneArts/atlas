#if DEBUG
  import CoreGraphics
  import Foundation

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

    /// A screenshot's thumbnail in the samples: the sample desktop at the samples' 2560 by 1440 captures' shape,
    /// known by the screenshot's name.
    ///
    /// - Parameter name: The screenshot's file name.
    /// - Returns: The thumbnail, or nil should the sample desktop not draw.
    @MainActor public static func shotPicture(_ name: String) -> DesktopPicture? {
      SampleDesktop.thumbnail(name, display: shotDisplay)
    }

    /// 2560 by 1440: the samples' captures' size, as the screenshots list says it.
    public static let shotDisplay = CGSize(width: 2560, height: 1440)

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
        aspect: shotDisplay.width / shotDisplay.height,
        isSelected: isSelected,
        label: "Screenshot taken at \(time)"
      )
    }

    /// A tile of a display's shape, light or dark, for the larger Screenshots samples.
    ///
    /// - Parameters:
    ///   - name: The screenshot's file name.
    ///   - time: When it was taken.
    ///   - display: The display's size, in points; the helper's 1512 by 982 by default.
    ///   - isDark: Whether the desktop is dark.
    ///   - isSelected: Whether it is selected.
    /// - Returns: The tile.
    @MainActor static func shotTile(
      named name: String,
      _ time: String,
      display: CGSize = helperDisplay,
      isDark: Bool = false,
      isSelected: Bool = false
    ) -> ViewerShotTile {
      ViewerShotTile(
        id: name,
        time: time,
        thumbnail: SampleDesktop.thumbnail(name, display: display, isDark: isDark),
        aspect: display.width / display.height,
        isSelected: isSelected,
        label: "Screenshot taken at \(time)"
      )
    }

    /// 1512 by 982: the helper's display in the samples, a MacBook Air's.
    static let helperDisplay = CGSize(width: 1512, height: 982)

    /// `count` tiles taken a few seconds apart from `start`, at the helper's shape, every fourth dark.
    ///
    /// - Parameters:
    ///   - group: The group's name, which their file names start with.
    ///   - count: How many.
    ///   - hour: The hour they were taken in.
    ///   - selected: The one selected, by its place; nil selects none.
    /// - Returns: The tiles, newest first.
    @MainActor static func shotRun(_ group: String, count: Int, hour: Int, selected: Int? = nil) -> [ViewerShotTile] {
      (0..<count)
        .map { index in
          let minute = 59 - index * 3
          let time = String(format: "%02d:%02d:%02d", hour, minute, 41 - index)
          return shotTile(
            named: "\(group)-\(index).png",
            time,
            isDark: index % 4 == 3,
            isSelected: index == selected
          )
        }
    }

    /// Screenshots with many taken, more than the pane holds, the last one of yesterday selected.
    @MainActor public static let viewerShotsMany: ViewerState = {
      var state = viewerShots
      state.sidebar.library = viewerLibrary(.screenshots, counts: [.screenshots: 26])
      state.pane = .screenshots(
        ViewerShotsState(
          meta: "18 today",
          days: [
            ViewerShotDay(id: "hour", title: "Now", shots: shotRun("now", count: 6, hour: 10)),
            ViewerShotDay(id: "morning", title: "Earlier today", shots: shotRun("today", count: 12, hour: 9)),
            ViewerShotDay(
              id: "yesterday",
              title: "Yesterday",
              shots: shotRun("yesterday", count: 8, hour: 17, selected: 7)
            ),
          ]
        )
      )
      return state
    }()

    /// Screenshots with a dark one selected among light ones.
    @MainActor public static let viewerShotsDarkSelected: ViewerState = {
      var state = viewerShots
      state.pane = .screenshots(
        ViewerShotsState(
          meta: "6 today",
          days: [
            ViewerShotDay(
              id: "hour",
              title: "Now",
              shots: [
                shotTile(named: "dark-0.png", "10:31:45"),
                shotTile(named: "dark-1.png", "10:31:44", isDark: true, isSelected: true),
                shotTile(named: "dark-2.png", "10:31:20", isDark: true),
                shotTile(named: "dark-3.png", "10:31:14"),
              ]
            ),
            ViewerShotDay(
              id: "morning",
              title: "Earlier today",
              shots: [
                shotTile(named: "dark-4.png", "10:12:03", isDark: true),
                shotTile(named: "dark-5.png", "09:58:41"),
              ]
            ),
          ]
        )
      )
      return state
    }()

    /// Screenshots of every shape: the helper's, 16 by 10, 4 by 3, 16 by 9, a tall window and an ultra-wide display,
    /// each drawn whole in its place.
    @MainActor public static let viewerShotsMixed: ViewerState = {
      var state = viewerShots
      state.pane = .screenshots(
        ViewerShotsState(
          meta: "7 today",
          days: [
            ViewerShotDay(
              id: "hour",
              title: "Now",
              shots: [
                shotTile(named: "mixed-0.png", "10:31:45"),
                shotTile(named: "mixed-1.png", "10:31:44", display: CGSize(width: 1024, height: 768), isSelected: true),
                shotTile(named: "mixed-2.png", "10:31:20", display: CGSize(width: 1920, height: 1080)),
                shotTile(named: "mixed-3.png", "10:31:14", display: CGSize(width: 800, height: 1100), isDark: true),
              ]
            ),
            ViewerShotDay(
              id: "morning",
              title: "Earlier today",
              shots: [
                shotTile(named: "mixed-4.png", "10:12:03", display: CGSize(width: 2560, height: 1080)),
                shotTile(named: "mixed-5.png", "09:58:41", display: CGSize(width: 1440, height: 900)),
                shotTile(named: "mixed-6.png", "09:40:12"),
              ]
            ),
          ]
        )
      )
      return state
    }()

    /// Screenshots an hour on, 10:31:44 selected, with the live screen in the corner.
    @MainActor public static let viewerShots: ViewerState = {
      var state = viewerIdle
      state.footer.latest = viewerStateLine("Idle", "Last: took a screenshot · 2 min ago")
      state.sidebar.library = viewerLibrary(.screenshots, counts: [.screenshots: 6])
      state.mini = SampleDesktop.picture
        .map { MiniScreenState(picture: $0) }
      state.pane = .screenshots(
        ViewerShotsState(
          meta: "6 today",
          days: [
            ViewerShotDay(
              id: "hour",
              title: "Now",
              shots: [
                shotTile(0, "10:31:45"), shotTile(1, "10:31:44", isSelected: true), shotTile(2, "10:31:20"),
                shotTile(3, "10:31:14"),
              ]
            ),
            ViewerShotDay(
              id: "morning",
              title: "Earlier today",
              shots: [shotTile(4, "10:12:03"), shotTile(5, "09:58:41")]
            ),
          ]
        )
      )
      return state
    }()

    /// 10:31:44 shown large over the whole window.
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

    /// Screenshots before any agent took one.
    @MainActor public static let viewerShotsEmpty: ViewerState = {
      var state = viewerShots
      state.sidebar.library = viewerLibrary(.screenshots, counts: [.screenshots: 0])
      state.pane = .screenshots(
        ViewerShotsState(
          meta: "",
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
