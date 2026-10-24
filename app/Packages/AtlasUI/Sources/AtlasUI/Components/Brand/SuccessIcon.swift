import SwiftUI

/// The app's icon as a success moment's hero (v3 D5, S8, X11): once per arrival it settles from 92% into place, and one
/// ring in the tile's shape expands from it and fades. A redraw never plays it again; only a new arrival, a new
/// identity, does. Under Reduce Motion the icon fades in and no ring is drawn; a render draws it settled, with no ring.
struct SuccessIcon: View {
  /// The visible tile's side, in points, such as the done screen's.
  let tile: CGFloat
  /// Called each time the moment plays, once per arrival.
  var played: @MainActor () -> Void = {}
  /// Whether the icon has settled into place.
  @State private var hasArrived = false
  /// Whether the ring has made its one pass.
  @State private var hasEchoed = false
  @Environment(\.animatesArrivals) private var animatesArrivals
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The icon, settling, under its ring.
  var body: some View {
    ZStack {
      AppIcon(tile: tile)
        .scaleEffect(iconScale)
        .opacity(isSettled ? 1 : 0)
      if animatesArrivals && !reduceMotion {
        echo
      }
    }
    .onAppear(perform: arrive)
    .accessibilityHidden(true)
  }

  /// The ring in the tile's shape: faint over the tile as the moment starts, then once out from it and away.
  private var echo: some View {
    RoundedRectangle(cornerRadius: tile * Layout.cornerRatio, style: .continuous)
      .stroke(Color.strokeEcho, lineWidth: Stroke.emphasis)
      .frame(width: tile, height: tile)
      .scaleEffect(hasEchoed ? Layout.echoScale : 1)
      .opacity(hasEchoed ? 0 : Layout.echoOpacity)
      .allowsHitTesting(false)
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

  /// Plays the moment once, as the icon arrives: it settles on ``Motion/settle``, and the ring makes its one pass on
  /// ``Motion/echo``. A render plays nothing.
  private func arrive() {
    guard animatesArrivals else { return }
    withAnimation(Motion.animation(Motion.settle, reduceMotion: reduceMotion, reduced: Motion.reduced)) {
      hasArrived = true
    }
    withAnimation(Motion.echo) { hasEchoed = true }
    played()
  }
}

/// v3's success moment (`celebrate`, `.hero .ring`).
private enum Layout {
  /// 0.224: the tile's corner radius against its side, Apple's icon shape.
  static let cornerRatio: CGFloat = 0.224
  /// 0.92: the size the icon settles from.
  static let arrivingScale: CGFloat = 0.92
  /// 1.35: the size the ring grows to as it fades.
  static let echoScale: CGFloat = 1.35
  /// 0.35: how much of the ring shows as it starts.
  static let echoOpacity = 0.35
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
