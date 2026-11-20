#if DEBUG
  import AppKit
  import AtlasService
  import AtlasUI

  /// The panel with two desktops, drawn from a scripted state rather than the live service, so the desktop popup and
  /// the menu bar's dot over every desktop can be checked offline. Debug builds only, beside the main window's renders.
  extension PanelRender {
    /// Each render's file name with the desktop it shows: Second Login, then the VM work.
    static let desktopRenders = [("panel-desktops-second-login", "second-login"), ("panel-desktops-work", "work")]

    /// Draws the panel with two desktops, one render a desktop shown, to PNGs in `directory`.
    ///
    /// - Parameters:
    ///   - directory: Where the PNGs go.
    ///   - appearance: The appearance to draw them in, light or dark.
    /// - Returns: Whether every render was drawn.
    static func drawDesktops(to directory: String, appearance: NSAppearance?) -> Bool {
      let overrides = PanelRenderOverrides(environment: ["ATLAS_RENDER_MODE": "agent", "ATLAS_RENDER_PREVIEW": "1"])
      return desktopRenders.map { file, shown in
        var snapshot = overrides.applied(to: PanelSnapshot(picture: SampleDesktop.picture))
        snapshot.desktops = desktops(shown: shown)
        return draw(snapshot, to: "\(directory)/\(file).png", appearance: appearance)
      }
      .allSatisfy { $0 }
    }

    /// Second Login idle and the VM work working, with `shown` the desktop shown.
    ///
    /// - Parameter shown: The ID of the desktop shown.
    /// - Returns: The list.
    static func desktops(shown: String) -> DesktopList {
      DesktopList(
        viewed: shown,
        desktops: [
          DesktopList.Entry(
            id: "second-login",
            key: "user",
            name: "second-login",
            kind: .user,
            isDefault: true,
            standing: .idle
          ),
          DesktopList.Entry(
            id: "work",
            key: "vm-work",
            name: "work",
            kind: .vm,
            standing: .working,
            facts: DesktopList.Facts(power: "running", maintenance: "none")
          ),
        ]
      )
    }
  }
#endif
