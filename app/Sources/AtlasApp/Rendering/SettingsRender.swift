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
      "29-screen-live",
      "30-screen-five-seconds",
      "31-screen-paused",
      "32-paused-unavailable",
      "33-notifications-on",
      "34-notifications-off",
      "35-startup-unavailable",
      "36-diagnostics",
      "37-diagnostics-mixed",
      "38-diagnostics-empty",
      "39-diagnostics-checking",
      "40-projects",
      "41-projects-empty",
      "42-projects-long-duplicates",
      "43-projects-many",
      "44-projects-forget-failed",
      "45-long-helper",
      "46-failed-connection",
      "47-claude-partial",
      "48-update-misplaced",
      "49-update-installed",
      "50-cleanup-failed",
      "51-search-empty",
      "52-search-ranked",
      "53-search-one",
      "54-search-none",
      "55-search-long-query",
      "56-search-long-breadcrumbs",
      "57-search-alert",
      "58-recover-refused",
      "59-recovered",
      "60-logging-out",
      "61-logging-out-anyway",
      "62-forgetting",
      "63-installing",
      "64-keyboard-focus",
    ]

    /// Every Settings state drawn for visual checks, by file name.
    static var renders: [(file: String, state: SettingsState)] {
      Array(zip(files, PreviewStates.settings.map(\.value)))
    }

    /// The accessible popup presentation for each existing cadence choice.
    static var popupRenders: [(file: String, state: SettingsState)] {
      renders.filter { ["29-screen-live", "30-screen-five-seconds", "31-screen-paused"].contains($0.file) }
        .map { ("popup-\($0.file)", $0.state) }
    }

    /// Each alert phase plus overflow and keyboard focus in explicit accessibility environments.
    static var accessibilityRenders: [(file: String, state: SettingsState)] {
      renders.filter { $0.state.alert != nil || ["45-long-helper", "64-keyboard-focus"].contains($0.file) }
    }

    /// Additional stable viewports; the original 28 state scenarios remain in their original order above.
    static var viewportRenders: [(file: String, state: SettingsState, size: NSSize)] {
      let minimum = NSSize(width: SettingsLayout.minimumWidth, height: SettingsLayout.minimumHeight)
      return Array(zip(files, renders))
        .map { file, render in
          ("minimum-\(file)", render.state, minimum)
        } + [
          (
            "long-helper", PreviewStates.settingsLongHelper,
            NSSize(width: SettingsLayout.defaultWidth, height: SettingsLayout.defaultHeight)
          ),
          (
            "tall-helper", renders[1].state,
            NSSize(width: SettingsLayout.defaultWidth, height: SettingsLayout.tallPreviewHeight)
          ),
        ]
    }

    /// Draws every Settings state to PNGs in `directory`, at Retina scale (``PNG/scale``), each at its window's size;
    /// with `ATLAS_RENDER_REDUCE_TRANSPARENCY=1`, as Reduce Transparency draws them, the alerts' glass opaque.
    ///
    /// - Parameter directory: Where the PNGs go.
    /// - Returns: Whether every state was drawn.
    static func draw(to directory: String) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      // A folder that cannot be made fails the writes below, which report it.
      try? FileManager.default
        .createDirectory(at: folder, withIntermediateDirectories: true)
      let reduceTransparency = ProcessInfo.processInfo.environment["ATLAS_RENDER_REDUCE_TRANSPARENCY"] == "1"
      let standard = renders.map { draw($0.file, $0.state, to: folder, reduceTransparency: reduceTransparency) }
      let viewports = viewportRenders.map {
        draw($0.file, $0.state, to: folder, reduceTransparency: reduceTransparency, size: $0.size)
      }
      let popups = popupRenders.map {
        draw($0.file, $0.state, to: folder, reduceTransparency: reduceTransparency, voiceOver: true)
      }
      let accessible = accessibilityRenders.flatMap { render in
        [
          draw("reduced-motion-\(render.file)", render.state, to: folder, reduceMotion: true),
          draw("reduced-transparency-\(render.file)", render.state, to: folder, reduceTransparency: true),
          draw("increased-contrast-\(render.file)", render.state, to: folder, contrast: .increased),
        ]
      }
      return (standard + viewports + popups + accessible)
        .allSatisfy { $0 }
    }

    /// Draws one Settings state to `file`.png in `folder`, at Retina scale, the stable Settings viewport.
    ///
    /// - Parameters:
    ///   - file: The PNG's name, without its extension.
    ///   - state: The state.
    ///   - folder: Where the PNG goes.
    ///   - reduceTransparency: Whether to draw it as under Reduce Transparency; not by default.
    ///   - size: The explicit viewport, or the stable default when nil.
    ///   - voiceOver: Whether to draw the accessible popup presentation; false by default.
    ///   - reduceMotion: Whether the fixture uses Reduce Motion; false by default.
    ///   - contrast: The fixture's contrast; standard by default.
    /// - Returns: Whether it was drawn.
    static func draw(
      _ file: String,
      _ state: SettingsState,
      to folder: URL,
      reduceTransparency: Bool = false,
      size: NSSize? = nil,
      voiceOver: Bool = false,
      reduceMotion: Bool = false,
      contrast: ColorSchemeContrast = .standard
    ) -> Bool {
      PNG.write(
        view(state) { _ in }
          .environment(\.settingsPicturePopup, voiceOver)
          .environment(\._accessibilityReduceMotion, reduceMotion)
          .environment(\._colorSchemeContrast, contrast)
          .environment(\._accessibilityReduceTransparency, reduceTransparency),
        size: size ?? Self.size(of: state),
        to: folder.appendingPathComponent("\(file).png")
      )
    }

    /// The stable default viewport, independent of pane content and status.
    ///
    /// - Parameter state: The state.
    /// - Returns: The size.
    static func size(of state: SettingsState) -> NSSize {
      NSSize(width: SettingsLayout.defaultWidth, height: SettingsLayout.defaultHeight)
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
