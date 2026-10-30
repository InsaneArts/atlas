#if DEBUG
  import SwiftUI

  /// The real shared capsule beside its tap and release frames, with unequal widths and quiet counts.
  struct TabsMotionLab: View {
    @State private var selected = ViewerActivityFilter.all
    @State private var reduced = false

    /// A live track for taps, drags, interruptions and keyboard input, followed by p = 0...1 frame series.
    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.m) {
        HStack(spacing: Spacing.s) {
          Button(String("All")) { selected = .all }
          Button(String("Screenshots")) { selected = .screenshots }
          Toggle(String("Reduce Motion"), isOn: $reduced)
        }
        control
          .environment(\.tabsReducedMotion, reduced)
        series(release: false)
        series(release: true)
      }
      .padding(Spacing.m)
    }

    /// Activity's unequal-width filters; this is the shared control, not a reproduction of its thumb.
    private var control: some View {
      Tabs(state: CapsuleTabsState(tabs: Self.tabs, selected: selected, label: "Activity filters"), style: .segment) {
        selected = $0
      }
    }

    /// A deterministic frame series in tenths, including both endpoints.
    private func series(release: Bool) -> some View {
      VStack(alignment: .leading, spacing: Spacing.s) {
        Text(release ? "Drag release · 180 pt/s" : "Tap slide · All to Screenshots")
          .font(Typography.font(.body, weight: .medium))
        ForEach(0...10, id: \.self) { step in
          HStack(spacing: Spacing.m) {
            Text(String(format: "p = %.1f", Double(step) / 10))
              .font(Typography.digits(.caption))
            control
              .environment(\.tabsMotionPose, TabsMotionPose(release: release, progress: Double(step) / 10))
              .allowsHitTesting(false)
          }
        }
      }
    }

    /// The sample filters deliberately span very different widths.
    private static let tabs = [
      CapsuleTab(id: ViewerActivityFilter.all, title: "All"),
      CapsuleTab(id: ViewerActivityFilter.problems, title: "Problems", count: 3),
      CapsuleTab(id: ViewerActivityFilter.typing, title: "Typing"),
      CapsuleTab(id: ViewerActivityFilter.screenshots, title: "Screenshots", count: 64),
    ]
  }

  #Preview("Segmented control motion") {
    ScrollView {
      TabsMotionLab()
    }
    .sampleSurface()
  }
#endif
