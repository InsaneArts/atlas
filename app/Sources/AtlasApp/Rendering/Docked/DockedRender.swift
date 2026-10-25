#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// Docked renders: the permission panel beside a sample System Settings window, tops aligned at its gap,
  /// with the drag ghost over both at one moment of its loop, or resting at the list under Reduce Motion.
  enum DockedRender {
    /// Each docked render, by file name: the ghost's moment in its loop, and whether Reduce Motion is on.
    static let renders: [(file: String, moment: TimeInterval, reduceMotion: Bool)] = [
      ("13-docked-ghost-appearing", 0.3, false),
      ("13-docked-ghost-mid-flight", 0.95, false),
      ("13-docked-reduce-motion", 0, true),
    ]
    /// The sample System Settings window's size.
    static let settings = CGSize(width: 680, height: 520)
    /// The margin around the two windows.
    static let margin: CGFloat = 40

    /// The canvas: both windows side by side, at their gap, within the margin.
    static var size: CGSize {
      let panel = OnboardingPlacement.size(guiding: true)
      return CGSize(
        width: margin * 2 + settings.width + OnboardingPlacement.settingsGap + panel.width,
        height: margin * 2 + max(settings.height, panel.height)
      )
    }

    /// Draws every docked render to a PNG named after it in `directory`.
    ///
    /// - Parameter directory: Where to write the PNGs.
    /// - Returns: Whether every PNG was written.
    static func draw(to directory: String) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      return renders.map { render in
        PNG.write(
          scene(moment: render.moment, reduceMotion: render.reduceMotion)
            .environment(\.appIcon, AppIconImage.image)
            .environment(\.accountPicture, AccountPictureImage.image),
          size: size,
          to: folder.appendingPathComponent("\(render.file).png")
        )
      }
      .allSatisfy { $0 }
    }

    /// The two windows and the ghost from the panel's icon to its landing in System Settings' list.
    ///
    /// - Parameters:
    ///   - moment: The ghost's moment in its loop.
    ///   - reduceMotion: Whether the ghost is drawn as Reduce Motion shows it.
    /// - Returns: The scene.
    static func scene(moment: TimeInterval, reduceMotion: Bool) -> some View {
      let panel = OnboardingPlacement.size(guiding: true)
      let target = CGPoint(
        x: margin + settings.width * DragGhost.landing.x,
        y: margin + settings.height * DragGhost.landing.y
      )
      return HStack(alignment: .top, spacing: OnboardingPlacement.settingsGap) {
        SettingsSketch(page: "Screen & System Audio Recording")
          .frame(width: settings.width, height: settings.height)
        OnboardingView(
          state: PreviewStates.onboardingPermissionPanel,
          form: .constant(PreviewStates.onboardingForm),
          dragIcon: AppIcon.dragSample
            .anchorPreference(key: DragIconAnchor.self, value: .bounds) { $0 }
        ) { _ in }
        .frame(width: panel.width, height: panel.height)
        .clipShape(RoundedRectangle(cornerRadius: 12))
      }
      .padding(margin)
      .frame(width: size.width, height: size.height, alignment: .topLeading)
      .background(Color.black)
      .overlayPreferenceValue(DragIconAnchor.self) { anchor in
        GeometryReader { proxy in
          if let anchor {
            let icon = proxy[anchor]
            DragGhostView(
              state: DragGhostState(from: CGPoint(x: icon.midX, y: icon.midY), to: target, start: .now),
              at: moment,
              reduceMotion: reduceMotion
            )
          }
        }
      }
    }
  }
#endif
