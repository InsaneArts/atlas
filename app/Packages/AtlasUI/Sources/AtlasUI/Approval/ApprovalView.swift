import SwiftUI

/// The approval window's content: what a command asks the person to allow, as one alert on a clear ground with room
/// around it for its shadow. The person allows or refuses it here, and nowhere else. The card fades in, settling from
/// 1.04 to 1 with no bounce as the window opens, or only fades in under Reduce Motion.
public struct ApprovalView: View {
  /// What the window shows.
  private let state: ApprovalState
  /// Receives the person's answer, or their closing the window.
  private let send: @MainActor (ApprovalChoice) -> Void
  /// Whether the card has settled since the window opened.
  @State private var settled = false
  @Environment(\.animatesArrivals) private var animatesArrivals
  @Environment(\.motionReduced) private var reduceMotion

  /// Creates the approval window's content.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - send: Receives the person's answer, or their closing the window.
  public init(state: ApprovalState, send: @escaping @MainActor (ApprovalChoice) -> Void) {
    self.state = state
    self.send = send
  }

  /// The request's alert, in the Mac's appearance, with room around it for its shadow.
  public var body: some View {
    AlertCard(state: state.card, send: send)
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
  #Preview("Add") {
    ApprovalView(state: PreviewStates.approvalAdd) { _ in }
      .sampleSurface()
  }

  #Preview("Remove") {
    ApprovalView(state: PreviewStates.approvalRemove) { _ in }
      .sampleSurface()
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.approvalAdd
    InteractivePreview {
      Menu(String("Approval")) {
        ForEach(PreviewStates.approval) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      ApprovalView(state: state) { choice in
        switch choice {
        case .allow: state.phase = .answered(allowed: true)
        case .refuse: state.phase = .answered(allowed: false)
        case .close: state.phase = .asking
        }
      }
    }
  }
#endif
