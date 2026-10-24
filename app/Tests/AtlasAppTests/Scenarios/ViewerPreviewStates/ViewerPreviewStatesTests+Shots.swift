#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  extension ViewerPreviewStatesTests {
    /// The Screenshots tab's samples' facts, by the sample's name: times on a German clock in UTC, for
    /// deterministic timestamps.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func shotsSamples() throws -> [String: ViewerState] {
      let shotStatus = try reported("observing", state: "idle", activity: Self.tookAScreenshot)
      return [
        "Screenshots": shown(shotStatus) { snapshot in
          Self.onShots(&snapshot)
        },
        "A screenshot, large": shown(shotStatus) { snapshot in
          Self.onShots(&snapshot)
          let name = PreviewStates.shotNames[1]
          snapshot.panes.shots.large = name
          snapshot.panes.shots.full = ViewerFullShot(
            png: Data(),
            picture: SampleDesktop.picture.map { DesktopPicture(id: "\(name) full", image: $0.image) }
          )
        },
        "Screenshots, none yet": shown(shotStatus) { snapshot in
          Self.onTab(.screenshots, at: Self.shotsNow, &snapshot)
          snapshot.panes.shots.shots = []
        },
      ]
    }

    /// The last thing an agent did: a screenshot two minutes ago.
    static let tookAScreenshot: [[String: Any]] = [
      ["operation": "screenshot", "says": "took a screenshot", "ok": true, "ageSeconds": 120]
    ]

    /// 11:13:00 on 22 October 2026, in UTC: when the Screenshots sample is looked at, an hour after 10:12.
    static let shotsNow = Date(timeIntervalSince1970: 1_792_667_580)

    /// The Screenshots sample's captures, newest first, each 2560 × 1440.
    static let shots: [Screenshot] = zip(
      PreviewStates.shotNames,
      [1_792_665_105, 1_792_665_104, 1_792_665_080, 1_792_665_074, 1_792_663_923, 1_792_663_121]
    )
    .map { Screenshot(name: $0, takenAt: Date(timeIntervalSince1970: $1), width: 2560, height: 1440) }

    /// The Screenshots tab with the sample's captures, 10:31:44 selected and every thumbnail read.
    ///
    /// - Parameter snapshot: The facts to change.
    static func onShots(_ snapshot: inout ViewerSnapshot) {
      onTab(.screenshots, at: shotsNow, &snapshot)
      snapshot.panes.shots.shots = shots
      snapshot.panes.shots.folder = URL(fileURLWithPath: "/Users/me/runs", isDirectory: true)
      snapshot.panes.shots.selected = PreviewStates.shotNames[1]
      keepThumbnails(&snapshot)
    }

    /// Every capture's thumbnail read: the sample desktop, known by the capture's name.
    ///
    /// - Parameter snapshot: The facts to change.
    static func keepThumbnails(_ snapshot: inout ViewerSnapshot) {
      guard let sample = SampleDesktop.picture else { return }
      for name in PreviewStates.shotNames {
        snapshot.panes.thumbnails.keep(DesktopPicture(id: name, image: sample.image), for: name)
      }
    }
  }
#endif
