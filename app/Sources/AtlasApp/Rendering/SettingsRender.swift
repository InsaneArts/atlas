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
      "22-helper-waiting",
      "23-helper-leftover",
      "24-helper-closing",
      "25-helper-still-logged-in",
      "26-claude-code-only",
      "27-claude-app-only",
      "28-claude-partly-connected",
    ]

    /// Every Settings state drawn for visual checks, by file name.
    static var renders: [(file: String, state: SettingsState)] {
      Array(zip(files, PreviewStates.settings.map(\.value)))
    }

    /// Draws every Settings state to PNGs in `directory`, at Retina scale (``PNG/scale``), each at its window's size.
    ///
    /// - Parameters:
    ///   - directory: Where the PNGs go.
    ///   - appearance: The appearance to draw them in, light or dark.
    /// - Returns: Whether every state was drawn.
    static func draw(to directory: String, appearance: NSAppearance?) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      // A folder that cannot be made fails the writes below, which report it.
      try? FileManager.default
        .createDirectory(at: folder, withIntermediateDirectories: true)
      let standard = renders.map {
        draw($0.file, $0.state, to: folder, appearance: appearance)
      }
      let narrow = PreviewStates.settingsAboutSamples
        .enumerated()
        .map { index, sample in
          draw(
            "about-narrow-\(index + 1)",
            sample.value,
            to: folder,
            appearance: appearance,
            width: PreviewStates.settingsNarrowWidth
          )
        }
      return (standard + narrow)
        .allSatisfy { $0 }
    }

    /// Draws one Settings state to `file`.png in `folder`, at Retina scale, Settings' width and its pane's
    /// height, as the window fits it.
    ///
    /// - Parameters:
    ///   - file: The PNG's name, without its extension.
    ///   - state: The state.
    ///   - folder: Where the PNG goes.
    ///   - appearance: The appearance to draw it in, light or dark.
    ///   - width: The window's width; the standard Settings width by default.
    /// - Returns: Whether it was drawn.
    static func draw(
      _ file: String,
      _ state: SettingsState,
      to folder: URL,
      appearance: NSAppearance?,
      width: CGFloat = SettingsLayout.width
    ) -> Bool {
      PNG.write(
        view(state) { _ in },
        size: size(of: state, width: width),
        to: folder.appendingPathComponent("\(file).png"),
        appearance: appearance
      )
    }

    /// The window's size for a state: Settings' width, and the title and tabs over the pane as it draws, which the
    /// pane reports as the window lays it out.
    ///
    /// - Parameters:
    ///   - state: The state.
    ///   - width: The available window width.
    /// - Returns: The size.
    static func size(of state: SettingsState, width: CGFloat = SettingsLayout.width) -> NSSize {
      var pane: CGFloat?
      let host = PNG.host(
        view(state) { event in
          if case .paneHeight(let tab, let height) = event, tab == state.tabs.selected { pane = height }
        }
      )
      host.frame = NSRect(x: 0, y: 0, width: width, height: SettingsLayout.firstHeight)
      host.layoutSubtreeIfNeeded()
      let height = pane.map { SettingsLayout.header + $0 } ?? SettingsLayout.firstHeight
      return NSSize(width: width, height: height.rounded(.up))
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
