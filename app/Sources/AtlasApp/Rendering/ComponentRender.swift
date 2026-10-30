#if DEBUG
  import AppKit
  import AtlasService
  import AtlasUI
  import SwiftUI

  /// The shared components' design renders: each component's samples, drawn at the size they take. Debug builds
  /// only, since the samples are.
  enum ComponentRender {
    /// Draws every component's samples to PNGs in `directory`, one per component (``ComponentSample``), at Retina
    /// scale.
    ///
    /// - Parameters:
    ///   - directory: Where the PNGs go.
    ///   - appearance: The appearance to draw them in, light or dark.
    /// - Returns: Whether every component was drawn.
    static func draw(to directory: String, appearance: NSAppearance?) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      // A folder that cannot be made fails the writes below, which report it.
      try? FileManager.default
        .createDirectory(at: folder, withIntermediateDirectories: true)
      let realIcons = Self.realIcons()
      return ComponentSample.allCases
        .map { sample in
          let view = ComponentSampleView(sample)
            .environment(\.appIcon, AppIconImage.image)
            .environment(\.accountPicture, AccountPictureImage.image)
            .environment(\.sampleAppIcons, realIcons)
          let size = PNG.host(view).fittingSize
          return PNG.write(
            view,
            size: size,
            to: folder.appendingPathComponent("\(sample.rawValue).png"),
            appearance: appearance
          )
        }
        .allSatisfy { $0 }
    }

    /// The system's own apps that every Mac has, whose icons the app glyph's samples stand beside.
    static let realApps = [
      "/System/Library/CoreServices/Finder.app",
      "/Applications/Safari.app",
      "/System/Applications/Notes.app",
      "/System/Applications/Calendar.app",
    ]

    /// Those apps' icons, read as the viewer reads an app's, at 128 pixels.
    ///
    /// - Returns: The icons that read, in order.
    static func realIcons() -> [IconImage] {
      realApps.compactMap { path in
        var rect = CGRect(x: 0, y: 0, width: AppIconCachePolicy.side, height: AppIconCachePolicy.side)
        guard let bundle = AppIconLookup.drawnBundle(path, home: NSHomeDirectory()),
          let image = NSWorkspace.shared
            .icon(forFile: bundle)
            .cgImage(forProposedRect: &rect, context: nil, hints: nil)
        else { return nil }
        return IconImage(id: path, image: image)
      }
    }
  }
#endif
