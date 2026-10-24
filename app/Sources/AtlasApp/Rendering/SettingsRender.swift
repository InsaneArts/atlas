#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// The Settings window's design renders. Debug builds only, since the samples are.
  enum SettingsRender {
    /// Each render's file name, in the order the previews list the samples (``PreviewStates/settings``): one for
    /// each sample. Kept apart from the states, which are the main actor's, so a test can list them as its arguments.
    nonisolated static let files = [
      "01-general",
      "02-helper",
      "03-agents",
      "04-about",
      "05-helper-not-ready",
      "06-agents-connecting",
      "07-agents-no-projects",
      "08-about-checking",
      "09-about-up-to-date",
      "10-about-newer",
      "11-about-failed",
      "12-recover",
      "13-log-out",
      "14-forget",
      "15-install",
      "16-switch",
      "17-log-out-anyway",
      "18-recovering",
      "19-about-installing",
      "20-about-install-failed",
      "21-about-unpublished",
    ]

    /// Every Settings state drawn for checking against the board, by file name.
    static var renders: [(file: String, state: SettingsState)] {
      Array(zip(files, PreviewStates.settings.map(\.value)))
    }

    /// Draws every Settings state to PNGs in `directory`, at Retina scale (``PNG/scale``), each at its window's size;
    /// with `ATLAS_RENDER_REDUCE_TRANSPARENCY=1`, as Reduce Transparency draws them, the alerts' glass opaque.
    ///
    /// - Parameter directory: Where the PNGs go.
    /// - Returns: Whether every state was drawn.
    static func draw(to directory: String) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      try? FileManager.default
        .createDirectory(at: folder, withIntermediateDirectories: true)
      let reduceTransparency = ProcessInfo.processInfo.environment["ATLAS_RENDER_REDUCE_TRANSPARENCY"] == "1"
      return renders.map { draw($0.file, $0.state, to: folder, reduceTransparency: reduceTransparency) }
        .allSatisfy { $0 }
    }

    /// Draws one Settings state to `file`.png in `folder`, at Retina scale, Settings' width and its pane's height, as
    /// the window fits it (v3 G2).
    ///
    /// - Parameters:
    ///   - file: The PNG's name, without its extension.
    ///   - state: The state.
    ///   - folder: Where the PNG goes.
    ///   - reduceTransparency: Whether to draw it as under Reduce Transparency; not by default.
    /// - Returns: Whether it was drawn.
    static func draw(_ file: String, _ state: SettingsState, to folder: URL, reduceTransparency: Bool = false) -> Bool {
      PNG.write(
        view(state) { _ in }
          .environment(\._accessibilityReduceTransparency, reduceTransparency),
        size: size(of: state),
        to: folder.appendingPathComponent("\(file).png")
      )
    }

    /// The window's size for a state: Settings' width, and the title and tabs over the pane as it draws, which the
    /// pane reports as the window lays it out.
    ///
    /// - Parameter state: The state.
    /// - Returns: The size.
    static func size(of state: SettingsState) -> NSSize {
      var pane: CGFloat?
      let host = PNG.host(
        view(state) { event in
          if case .paneHeight(let tab, let height) = event, tab == state.tabs.selected { pane = height }
        }
      )
      host.frame = NSRect(x: 0, y: 0, width: SettingsLayout.width, height: SettingsLayout.firstHeight)
      host.layoutSubtreeIfNeeded()
      let height = pane.map { SettingsLayout.header + $0 } ?? SettingsLayout.firstHeight
      return NSSize(width: SettingsLayout.width, height: height.rounded(.up))
    }

    /// The window's view for a render.
    ///
    /// - Parameters:
    ///   - state: The state.
    ///   - send: Receives the view's events, such as its pane's height.
    /// - Returns: The view, with the app's icon.
    private static func view(
      _ state: SettingsState,
      send: @escaping @MainActor (SettingsEvent) -> Void
    ) -> some View {
      SettingsView(state: state, edit: { _ in }, send: send)
        .environment(\.appIcon, AppIconImage.image)
        .environment(\.accountPicture, AccountPictureImage.image)
    }
  }
#endif
