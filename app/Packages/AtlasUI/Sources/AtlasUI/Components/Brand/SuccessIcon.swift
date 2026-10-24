import SwiftUI

/// The app's icon as a success moment's hero: once per arrival it settles from 92% into place, and nothing else plays
/// around it. A redraw never plays it again; only a new arrival, a new identity, does. Under Reduce Motion the icon
/// fades in; a render draws it settled.
struct SuccessIcon: View {
  /// The visible tile's side, in points, such as the done screen's.
  let tile: CGFloat
  /// Called each time the moment plays, once per arrival.
  var played: @MainActor () -> Void = {}
  /// Whether the icon has settled into place.
  @State private var hasArrived = false
  @Environment(\.animatesArrivals) private var animatesArrivals
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The icon, settling.
  var body: some View {
    AppIcon(tile: tile)
      .scaleEffect(iconScale)
      .opacity(isSettled ? 1 : 0)
      .onAppear(perform: arrive)
      .accessibilityHidden(true)
  }

  /// Whether the icon shows whole: once it has arrived, or at once where nothing plays.
  private var isSettled: Bool {
    hasArrived || !animatesArrivals
  }

  /// The icon's scale: 92% until it settles; whole under Reduce Motion, where it only fades.
  private var iconScale: CGFloat {
    guard !isSettled, !reduceMotion else { return 1 }
    return Layout.arrivingScale
  }

  /// Plays the moment once, as the icon arrives: it settles on ``Motion/settle``. A render plays nothing.
  private func arrive() {
    guard animatesArrivals else { return }
    withAnimation(Motion.animation(Motion.settle, reduceMotion: reduceMotion, reduced: Motion.reduced)) {
      hasArrived = true
    }
    played()
  }
}

/// The success moment's measures.
private enum Layout {
  /// 0.92: the size the icon settles from.
  static let arrivingScale: CGFloat = 0.92
}

#if DEBUG
  #Preview("Settled") {
    SuccessIcon(tile: OnboardingLayout.doneTile)
      .environment(\.animatesArrivals, false)
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var arrival = 0
    InteractivePreview {
      Button(String("Arrive again")) { arrival += 1 }
    } content: {
      SuccessIcon(tile: OnboardingLayout.doneTile)
        .id(arrival)
    }
  }
#endif
