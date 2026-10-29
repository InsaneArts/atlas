import SwiftUI

/// The alert window's content: one alert on a clear ground, with room around it for its shadow. Atlas shows it when
/// the menu bar panel asks to recover, switch to or log out the helper while no main window is open; the main window
/// shows the same alert over its own scrim instead. The card fades in, settling from 1.04 to 1 with no bounce as the
/// window opens, or only fades in under Reduce Motion.
public struct AlertView: View {
  /// What the alert says and offers.
  private let state: AlertCardState<AlertChoice>
  /// Receives the button the person presses.
  private let send: @MainActor (AlertChoice) -> Void
  /// Whether the card has settled since the window opened.
  @State private var settled = false
  @Environment(\.animatesArrivals) private var animatesArrivals
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates the alert window's content.
  ///
  /// - Parameters:
  ///   - state: What the alert says and offers.
  ///   - send: Receives the button the person presses.
  public init(state: AlertCardState<AlertChoice>, send: @escaping @MainActor (AlertChoice) -> Void) {
    self.state = state
    self.send = send
  }

  /// The alert, in the Mac's appearance, with room around it for its shadow.
  public var body: some View {
    AlertCard(state: state, send: send)
      .scaleEffect(AlertCardScale.arrival(settled: shown, reduceMotion: reduceMotion))
      .opacity(shown ? 1 : .zero)
      .padding(AlertLayout.shadowRoom)
      .foregroundStyle(Color.textPrimary)
      .onAppear {
        withAnimation(Motion.animation(Motion.arrive, reduceMotion: reduceMotion, reduced: Motion.reduced)) {
          settled = true
        }
      }
  }

  /// Whether the card shows settled: once it has, or at once where arrivals are not played, as in a render.
  private var shown: Bool {
    settled || !animatesArrivals
  }
}

#if DEBUG
  #Preview("Recover") {
    AlertView(state: PreviewStates.alertRecover) { _ in }
      .sampleSurface()
  }

  #Preview("Log out") {
    AlertView(state: PreviewStates.alertLogOut) { _ in }
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.alertRecover
    InteractivePreview {
      Menu(String("Alert")) {
        ForEach(PreviewStates.alert) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      AlertView(state: state) { choice in
        state.phase = choice == .confirm ? .running(label: AlertCardSamples.recovering) : .asking
      }
    }
  }
#endif
