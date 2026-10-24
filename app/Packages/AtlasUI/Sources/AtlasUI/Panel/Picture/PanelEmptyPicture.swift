import SwiftUI

/// The panel's picture frame with no screen to show (v3 P6): Atlas's mark on the empty stage over one quiet line saying
/// why. While connecting, the mark alone breathes, only after ``Motion/Duration/appearDelay`` so a quick answer never
/// flashes it; under Reduce Motion it holds still. VoiceOver reads the line.
struct PanelEmptyPicture: View {
  /// The line, such as "The helper is logged out"; empty for the mark alone.
  let message: String
  /// Whether the mark breathes, while connecting.
  let breathes: Bool
  /// Whether the breathing mark has come in after its delay.
  @State private var isShown = false
  /// Whether the breath is at its high point; it swings there and back for as long as it lasts.
  @State private var isInhaled = false
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.animatesArrivals) private var animatesArrivals

  /// Atlas's mark over the line saying why there is no screen.
  var body: some View {
    VStack(spacing: Spacing.m) {
      OrbitMark(size: PanelLayout.emptyMark)
        .foregroundStyle(Color.textPrimary)
        .opacity(Self.markOpacity(breathes: breathes, isInhaled: isInhaled, isStill: isStill))
        .opacity(breathes && animatesArrivals && !isShown ? 0 : 1)
      if !message.isEmpty {
        Text(message)
          .textStyle(.callout)
          .foregroundStyle(Color.textTertiary)
          .multilineTextAlignment(.center)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
    .padding(.horizontal, PanelLayout.emptyInset)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background {
      RoundedRectangle(cornerRadius: Radius.medium)
        .filledOutline(Color.fillPlaceholder, border: Color.separator)
    }
    .accessibilityElement(children: .combine)
    .onAppear(perform: breathe)
    .onChange(of: breathes) { breathe() }
    .onChange(of: reduceMotion) { breathe() }
  }

  /// Whether the breath holds still: under Reduce Motion, and in a render, which draws one frame.
  private var isStill: Bool {
    !Motion.loops(reduceMotion: reduceMotion) || !animatesArrivals
  }

  /// The mark's opacity: at rest on an empty stage that says why; while connecting, swinging between v3's low and high
  /// points, or held at the still point.
  ///
  /// - Parameters:
  ///   - breathes: Whether the mark breathes, while connecting.
  ///   - isInhaled: Whether the breath is at its high point.
  ///   - isStill: Whether the breath holds still.
  /// - Returns: The opacity.
  nonisolated static func markOpacity(breathes: Bool, isInhaled: Bool, isStill: Bool) -> Double {
    guard breathes else { return PanelLayout.markRest }
    if isStill { return PanelLayout.breathStill }
    return isInhaled ? PanelLayout.breathHigh : PanelLayout.breathLow
  }

  /// Brings the mark in after its delay and starts its breath while connecting and motion is welcome; otherwise rests
  /// it. The waits are the animations' own delays, never a sleep.
  private func breathe() {
    var still = Transaction()
    still.disablesAnimations = true
    withTransaction(still) {
      isShown = false
      isInhaled = false
    }
    guard breathes else { return }
    withAnimation(Motion.appear) { isShown = true }
    guard !isStill else { return }
    withAnimation(Motion.breathe.delay(Motion.Duration.appearDelay.seconds)) { isInhaled = true }
  }
}

#if DEBUG
  #Preview("Not ready") {
    PanelEmptyPicture(
      message: String("Atlas checks again by itself. Settings shows what the helper needs"),
      breathes: false
    )
    .frame(width: PanelLayout.picture.width, height: PanelLayout.picture.height)
    .sampleSurface()
  }

  #Preview("Connecting") {
    PanelEmptyPicture(message: String(), breathes: true)
      .frame(width: PanelLayout.picture.width, height: PanelLayout.picture.height)
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var breathes = true
    @Previewable @State var attempt = 0
    InteractivePreview {
      Toggle(String("Connecting"), isOn: $breathes)
      Button(String("Arrive again")) { attempt += 1 }
    } content: {
      PanelEmptyPicture(message: breathes ? String() : String("The helper is logged out"), breathes: breathes)
        .frame(width: PanelLayout.picture.width, height: PanelLayout.picture.height)
        .id(attempt)
    }
  }
#endif
