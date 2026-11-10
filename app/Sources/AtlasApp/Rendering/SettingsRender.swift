#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// The Settings window's design renders. Debug builds only, since the samples are.
  enum SettingsRender {
    /// Each render's file name, in the order the previews list the samples (``PreviewStates/settings``): one for
    /// each sample. Kept apart from the states, which are the main actor's, so a test can list them as its arguments.
    nonisolated static let files = [
      "01-helper",
      "02-general",
      "03-activity",
      "04-accounts",
      "05-privacy",
      "06-integrations",
      "07-about",
      "08-activity-custom",
      "09-helper-not-ready",
      "10-integrations-connecting",
      "11-integrations-no-projects",
      "12-about-checking",
      "13-about-up-to-date",
      "14-about-newer",
      "15-about-failed",
      "16-recover",
      "17-log-out",
      "18-forget",
      "19-install",
      "20-switch",
      "21-log-out-anyway",
      "22-recovering",
      "23-about-installing",
      "24-about-install-failed",
      "25-about-unpublished",
      "26-helper-waiting",
      "27-helper-leftover",
      "28-helper-closing",
      "29-helper-still-logged-in",
      "30-claude-code-only",
      "31-claude-app-only",
      "32-claude-partly-connected",
      "33-privacy-home-open",
      "34-privacy-home-failed",
      "35-privacy-copies-allowed",
      "36-open-home",
      "37-allow-sign-ins",
      "38-accounts-none",
      "39-accounts-added",
      "40-accounts-unread",
      "41-remove-account",
      "42-accounts-look-only",
      "43-sign-in",
      "44-sign-in-refused",
      "45-account-opened",
      "46-accounts-pictures-off",
    ]

    /// The window's size: Settings' fixed 740 by 572.
    nonisolated static let size = NSSize(width: SettingsLayout.width, height: SettingsLayout.height)

    /// Every Settings state drawn for visual checks, by file name.
    static var renders: [(file: String, state: SettingsState)] {
      Array(zip(files, PreviewStates.settings.map(\.value)))
    }

    /// Draws every Settings state to PNGs in `directory`, at Retina scale (``PNG/scale``), each at the window's size.
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
          draw("about-\(index + 1)", sample.value, to: folder, appearance: appearance)
        }
      return (standard + narrow)
        .allSatisfy { $0 }
    }

    /// Draws one Settings state to `file`.png in `folder`, at Retina scale and the window's size.
    ///
    /// - Parameters:
    ///   - file: The PNG's name, without its extension.
    ///   - state: The state.
    ///   - folder: Where the PNG goes.
    ///   - appearance: The appearance to draw it in, light or dark.
    /// - Returns: Whether it was drawn.
    static func draw(_ file: String, _ state: SettingsState, to folder: URL, appearance: NSAppearance?) -> Bool {
      PNG.write(
        view(state),
        size: size,
        to: folder.appendingPathComponent("\(file).png"),
        appearance: appearance
      )
    }

    /// The window's view for a render.
    ///
    /// - Parameter state: The state.
    /// - Returns: The view, with the app's icon.
    private static func view(_ state: SettingsState) -> some View {
      SettingsView(state: state, edit: { _ in }, send: { _ in })
        .environment(\.appIcon, AppIconImage.image)
        .environment(\.accountPicture, AccountPictureImage.image)
    }
  }
#endif
