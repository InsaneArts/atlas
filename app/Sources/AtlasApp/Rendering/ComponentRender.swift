#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// The shared components' design renders: each component's samples, drawn at the size they take. Debug builds
  /// only, since the samples are.
  enum ComponentRender {
    /// Draws every component's samples to PNGs in `directory`, one per component (``ComponentSample``), at Retina
    /// scale.
    ///
    /// - Parameter directory: Where the PNGs go.
    /// - Returns: Whether every component was drawn.
    static func draw(to directory: String) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      try? FileManager.default
        .createDirectory(at: folder, withIntermediateDirectories: true)
      return ComponentSample.allCases
        .map { sample in
          let view = ComponentSampleView(sample)
            .environment(\.appIcon, AppIconImage.image)
          let size = WindowShell.hostingView(view).fittingSize
          return PNG.write(view, size: size, to: folder.appendingPathComponent("\(sample.rawValue).png"))
        }
        .allSatisfy { $0 }
    }
  }
#endif
