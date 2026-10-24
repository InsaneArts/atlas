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

    /// Draws every Settings state to PNGs in `directory`, at Retina scale (``PNG/scale``) and the window's size.
    ///
    /// - Parameter directory: Where the PNGs go.
    /// - Returns: Whether every state was drawn.
    static func draw(to directory: String) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      try? FileManager.default
        .createDirectory(at: folder, withIntermediateDirectories: true)
      return renders.map { draw($0.file, $0.state, to: folder) }
        .allSatisfy { $0 }
    }

    /// Draws one Settings state to `file`.png in `folder`, at Retina scale and the window's size.
    ///
    /// - Parameters:
    ///   - file: The PNG's name, without its extension.
    ///   - state: The state.
    ///   - folder: Where the PNG goes.
    /// - Returns: Whether it was drawn.
    static func draw(_ file: String, _ state: SettingsState, to folder: URL) -> Bool {
      PNG.write(
        SettingsView(state: state, edit: { _ in }, send: { _ in })
          .environment(\.appIcon, AppIconImage.image),
        size: SettingsLayout.window,
        to: folder.appendingPathComponent("\(file).png")
      )
    }
  }
#endif
