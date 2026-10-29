import SwiftUI

/// The content card's footer, 46 points high under the stage: the newest thing to know at its leading edge, in line
/// with the header's words, cut short first, and the buttons at its trailing edge, so nothing covers the screen. In a
/// narrow card Stop and Resume turn into a circle round their symbol and Take Control into its hand, their titles fading
/// out on ``Motion/swapOut`` before they narrow on ``Motion/state``, and back the other way as it widens; narrower
/// still, the line keeps only its session's icon and its age. A change of mind mid-way turns back from where it is.
/// Under Reduce Motion the circles change at once and only the words crossfade. Only these steps move: the window's own
/// resize never animates anything here, and a press never does.
struct ViewerFooter: View {
  /// What the footer shows.
  let state: ViewerFooterState
  /// How much of its room the line holds back at its trailing end while the sidebar moves (``SidebarSlide``).
  let hold: CGFloat
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  /// How the footer draws now, which follows its fit step by step.
  @State private var fold: FooterFold
  /// Each change calls off the step an earlier one left to finish.
  @State private var generation = 0
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates the footer.
  ///
  /// - Parameters:
  ///   - state: What the footer shows.
  ///   - hold: How much of its room the line holds back while the sidebar moves; none by default.
  ///   - send: Receives what the person does.
  init(state: ViewerFooterState, hold: CGFloat = .zero, send: @escaping @MainActor (ViewerEvent) -> Void) {
    self.state = state
    self.hold = hold
    self.send = send
    _fold = State(initialValue: FooterFold(state.fit))
  }

  /// The line and the buttons.
  var body: some View {
    HStack(spacing: ViewerLayout.footerGap) {
      FooterLatest(latest: state.latest, showsWords: fold.showsWords, hold: hold)
      FooterControls(controls: state.controls, fold: fold, send: send)
    }
    .padding(.leading, ViewerLayout.footerLeading)
    .padding(.trailing, ViewerLayout.footerTrailing)
    .frame(height: ViewerLayout.footerHeight)
    .onChange(of: state.fit) { _, fit in refold(to: fit) }
  }

  /// Moves the footer to `fit`: the line's words fade, and the capsules turn into circles or back in their two steps.
  ///
  /// - Parameter fit: How the footer gives way now.
  private func refold(to fit: ViewerFooterFit) {
    generation += 1
    let current = generation
    let target = FooterFold(fit)
    guard !reduceMotion else {
      var still = Transaction()
      still.disablesAnimations = true
      withTransaction(still) { fold.isCircles = target.isCircles }
      withAnimation(Motion.crossfadeReduced) {
        fold.showsTitles = target.showsTitles
        fold.showsWords = target.showsWords
      }
      return
    }
    if target.showsWords != fold.showsWords {
      withAnimation(target.showsWords ? Motion.swapIn : Motion.swapOut) { fold.showsWords = target.showsWords }
    }
    if target.isCircles {
      narrow(generation: current)
    } else {
      widen(generation: current)
    }
  }

  /// Narrows the capsules into circles: their titles fade out first, then they narrow.
  ///
  /// - Parameter current: The change's generation, which a newer change outdates.
  private func narrow(generation current: Int) {
    guard fold.showsTitles else {
      withAnimation(ViewerLayout.footerFoldMotion(reduceMotion: false)) { fold.isCircles = true }
      return
    }
    withAnimation(Motion.swapOut) {
      fold.showsTitles = false
    } completion: {
      guard generation == current else { return }
      withAnimation(ViewerLayout.footerFoldMotion(reduceMotion: false)) { fold.isCircles = true }
    }
  }

  /// Opens the circles into capsules: they widen first, then their titles fade in.
  ///
  /// - Parameter current: The change's generation, which a newer change outdates.
  private func widen(generation current: Int) {
    guard fold.isCircles else {
      if !fold.showsTitles { withAnimation(Motion.swapIn) { fold.showsTitles = true } }
      return
    }
    withAnimation(ViewerLayout.footerFoldMotion(reduceMotion: false)) {
      fold.isCircles = false
    } completion: {
      guard generation == current else { return }
      withAnimation(Motion.swapIn) { fold.showsTitles = true }
    }
  }
}

#if DEBUG
  #Preview("Watching and in control") {
    VStack(spacing: Spacing.m) {
      ViewerFooter(state: PreviewStates.viewerWorking.footer) { _ in }
      ViewerFooter(state: PreviewStates.viewerControl.footer) { _ in }
      ViewerFooter(state: PreviewStates.viewerStopped.footer) { _ in }
      ViewerFooter(state: PreviewStates.viewerNeedsYou.footer) { _ in }
    }
    .frame(width: SampleWidth.column * 3)
    .background(Color.fillContent)
    .sampleSurface()
  }

  #Preview("Narrow") {
    @Previewable @State var fit = ViewerFooterFit.full
    InteractivePreview {
      Picker(String("Fit"), selection: $fit) {
        Text(verbatim: "Full")
          .tag(ViewerFooterFit.full)
        Text(verbatim: "Compact")
          .tag(ViewerFooterFit.compact)
        Text(verbatim: "Tight")
          .tag(ViewerFooterFit.tight)
      }
      .pickerStyle(.segmented)
      .fixedSize()
    } content: {
      let narrowed: (ViewerState) -> ViewerFooterState = { sample in
        var footer = sample.footer
        footer.fit = fit
        return footer
      }
      VStack(spacing: Spacing.m) {
        ViewerFooter(state: narrowed(PreviewStates.viewerWorking)) { _ in }
        ViewerFooter(state: narrowed(PreviewStates.viewerStopped)) { _ in }
        ViewerFooter(state: narrowed(PreviewStates.viewerControl)) { _ in }
      }
      .frame(width: SampleWidth.column * 2)
      .background(Color.fillContent)
    }
  }
#endif
