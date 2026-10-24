#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  extension ViewerPreviewStatesTests {
    /// The Apps tab's samples' facts, by the sample's name.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func appsSamples() throws -> [String: ViewerState] {
      let idle = try reported("observing", state: "idle")
      return [
        "Apps, paused": shown(idle) { snapshot in
          snapshot.session.tab = .apps
          snapshot.session.paused = true
          snapshot.panes.apps.apps = Self.apps
          snapshot.panes.apps.icons = SampleAppIcons.icons
        },
        "Apps": shown(idle) { snapshot in
          snapshot.session.tab = .apps
          snapshot.panes.apps.apps = Self.apps
          snapshot.panes.apps.icons = SampleAppIcons.icons
        },
        "Show on screen": shown(idle) { snapshot in
          snapshot.reading.frame?.display = ViewerStatus.Size(width: 1728, height: 1080)
          snapshot.panes.apps.icons = SampleAppIcons.icons
          snapshot.panes.apps.shown = ViewerShownWindow(
            app: "Opie",
            icon: SampleAppIcons.opie,
            window: HelperWindow(id: 12, title: "Ring settings", x: 1064, y: 80, width: 520, height: 860)
          )
        },
        "Show on screen, at the right edge": shown(idle) { snapshot in
          snapshot.reading.frame?.display = ViewerStatus.Size(width: 1728, height: 1080)
          snapshot.panes.apps.icons = SampleAppIcons.icons
          snapshot.panes.apps.shown = ViewerShownWindow(
            app: "Opie",
            icon: SampleAppIcons.opie,
            window: HelperWindow(
              id: 13,
              title: "Ring settings for the appearance of every ring and slice",
              x: 1428,
              y: 80,
              width: 300,
              height: 600
            )
          )
        },
      ]
    }

    /// The Apps sample's apps, front to back (MacApps).
    static let apps = [
      HelperApp(
        pid: 501,
        name: "Opie",
        bundle: SampleAppIcons.opie,
        front: true,
        windows: [
          HelperWindow(id: 11, title: "Starter", width: 1080, height: 590),
          HelperWindow(id: 12, title: "Ring settings", width: 520, height: 860),
        ]
      ),
      HelperApp(
        pid: 502,
        name: "Safari",
        bundle: SampleAppIcons.safari,
        windows: [HelperWindow(id: 21, title: "Opie — Release notes", width: 1200, height: 780)]
      ),
      HelperApp(
        pid: 503,
        name: "Notes",
        bundle: SampleAppIcons.notes,
        windows: [HelperWindow(id: 31, title: "QA checklist", width: 900, height: 640)]
      ),
      HelperApp(
        pid: 504,
        name: "Terminal",
        bundle: SampleAppIcons.terminal,
        windows: [HelperWindow(id: 41, title: "atlas — zsh", width: 720, height: 460)]
      ),
      HelperApp(pid: 505, name: "Finder", bundle: SampleAppIcons.finder),
    ]
  }
#endif
