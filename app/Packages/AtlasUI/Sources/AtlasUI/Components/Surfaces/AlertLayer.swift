import SwiftUI

/// An alert over a whole window, top row and tabs included: the scrim, which takes every click, and the alert in its
/// middle, fading in and out together, or appearing at once under Reduce Motion.
struct AlertLayer<ID: Hashable & Sendable>: View {
  /// What the alert says and offers; nil shows none.
  let state: OrbitAlertState<ID>?
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    ZStack {
      if let state {
        ZStack {
          Color.scrim
          OrbitAlert(state: state, send: send)
        }
        .transition(reduceMotion ? .identity : .opacity)
      }
    }
    .animation(reduceMotion ? nil : Motion.fade, value: state == nil)
  }
}

#if DEBUG
  #Preview("Samples") {
    AlertLayer(state: OrbitAlertSamples.logOut(isBusy: false)) { _ in }
      .frame(width: SampleWidth.form, height: SampleWidth.form)
  }

  #Preview("Interactive") {
    @Previewable @State var shown = true
    InteractivePreview {
      Toggle(String("Shown"), isOn: $shown)
    } content: {
      AlertLayer(state: shown ? OrbitAlertSamples.logOut(isBusy: false) : nil) { _ in shown = false }
        .frame(width: SampleWidth.form, height: SampleWidth.form)
    }
  }
#endif
