#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// Opie's Ring settings window outlined on the screen, from the Apps list: 520 × 860 at 1064, 80 on a 1728
    /// × 1080 display.
    @MainActor public static let viewerShowOnScreen: ViewerState = {
      var state = viewerIdle
      state.footer.latest = viewerStateLine("Idle", "Showing Opie's window")
      state.screen.outline = ViewerWindowOutline(
        frame: CGRect(
          x: 1064.0 / 1728,
          y: 80.0 / 1080,
          width: 1584.0 / 1728 - 1064.0 / 1728,
          height: 940.0 / 1080 - 80.0 / 1080
        ),
        title: "Opie · Ring settings",
        size: "520 × 860",
        icon: .of(SampleAppIcons.icons[SampleAppIcons.opie], name: "Opie")
      )
      return state
    }()

    /// A window docked at the right edge with a long name, outlined: its label keeps inside the picture and cuts the
    /// name short. 300 × 600 at 1428, 80 on a 1728 × 1080 display.
    @MainActor public static let viewerShowOnScreenRightEdge: ViewerState = {
      var state = viewerShowOnScreen
      state.screen.outline = ViewerWindowOutline(
        frame: CGRect(
          x: 1428.0 / 1728,
          y: 80.0 / 1080,
          width: 1728.0 / 1728 - 1428.0 / 1728,
          height: 680.0 / 1080 - 80.0 / 1080
        ),
        title: "Opie · Ring settings for the appearance of every ring and slice",
        size: "300 × 600",
        icon: .of(SampleAppIcons.icons[SampleAppIcons.opie], name: "Opie")
      )
      return state
    }()
  }
#endif
