import SwiftUI

/// The main window in the widget's look: one quiet row at the top, the helper's screen under it, the Details column
/// beside it on request, and an alert over everything when one is asked. It draws its state and sends what the person
/// does; it decides nothing. The shell supplies `input`, the surface over the screen that takes the mouse and
/// keyboard while in control, so this view never sees AppKit; previews pass none.
public struct ViewerView<Input: View>: View {
  /// What the window shows.
  private let state: ViewerState
  /// Receives what the person does.
  private let send: @MainActor (ViewerEvent) -> Void
  /// Reads how old the newest picture is, as Details' row draws.
  private let newest: @MainActor () -> String
  /// The surface laid over the picture that takes the mouse and keyboard.
  private let input: Input
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates the window's content.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - send: Receives what the person does.
  ///   - newest: Reads how old the newest picture is as Details' row draws: the app's reads its controller, whose
  ///     clock ticks every second, so the picture's age redraws that row alone and not the window.
  ///   - input: The surface laid over the picture that takes the mouse and keyboard.
  public init(
    state: ViewerState,
    send: @escaping @MainActor (ViewerEvent) -> Void,
    newest: @escaping @MainActor () -> String,
    @ViewBuilder input: () -> Input
  ) {
    self.state = state
    self.send = send
    self.newest = newest
    self.input = input()
  }

  /// The top row in the title bar, the screen or a tab's pane under it, Details beside them, the mini screen in the
  /// corner, a screenshot shown large over all of it, and the alert over that.
  public var body: some View {
    GeometryReader { proxy in
      VStack(spacing: .zero) {
        // The top row fills the title bar, whatever its height.
        ViewerTopRow(state: state.top, fit: ViewerLayout.topRowFit(width: proxy.size.width), send: send)
          .frame(height: max(proxy.safeAreaInsets.top, ViewerLayout.topRowHeight))
        HStack(spacing: .zero) {
          // Every tab stands in the same frame, the content area less the window's inset: the panes fill it, the picture
          // fits in it.
          pane
            .padding(Spacing.m)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            // The small screen keeps to the panes' corner, clear of Details.
            .overlay(alignment: .bottomTrailing) {
              if let mini = state.mini {
                MiniScreen(state: mini) { send(.selectTab(.screen)) }
                  .padding(ViewerLayout.miniInset)
              }
            }
          // Details is a pane of the same card beside the main one, below the top row: its top level with the main
          // pane's, the window's inset to its right and below it, the main pane's own inset as the gap between them.
          // The slide moves Details alone: the pane takes its new width at once, so nothing on the screen, a caption,
          // the outline or the picture's frame, rides along.
          ZStack {
            if let details = state.details {
              ViewerDetails(state: details, newest: newest, send: send)
                .frame(width: ViewerLayout.detailsWidth)
                .padding([.top, .bottom, .trailing], Spacing.m)
                .transition(
                  reduceMotion
                    ? .opacity
                    : .move(edge: .trailing)
                      .combined(with: .opacity)
                )
            }
          }
          .animation(
            Motion.animation(Motion.slide, reduceMotion: reduceMotion, reduced: Motion.fade),
            value: state.details == nil
          )
        }
      }
      .overlay {
        ZStack {
          if let large = largeShot {
            LargeShotLayer(shot: large, send: send)
              .transition(reduceMotion ? .identity : .opacity)
          }
        }
        .animation(Motion.animation(Motion.fade, reduceMotion: reduceMotion), value: largeShot == nil)
      }
      .overlay {
        AlertLayer(state: state.alert) { send($0 == .confirm ? .alertConfirm : .alertCancel) }
      }
      .ignoresSafeArea(.container, edges: .top)
    }
    .windowSurface()
    .onChange(of: state.top.status.label) { _, label in announce(label) }
    .onChange(of: state.screen.notice) { _, notice in if let notice { announce(notice) } }
    .onChange(of: state.alert?.title) { _, title in if let title { announce(title) } }
  }

  /// The Screen tab, or the selected tab's pane: Activity, Apps and Screenshots fill the frame inside the window's
  /// inset, and the Screen's picture fits within that same frame at its own shape.
  @ViewBuilder private var pane: some View {
    switch state.pane {
    case .screen:
      ViewerScreen(state: state.screen, send: send, input: input)
    case .activity(let activity):
      ViewerActivityPane(state: activity, send: send)
    case .apps(let apps):
      ViewerAppsPane(state: apps, send: send)
    case .screenshots(let shots):
      ViewerShotsPane(state: shots, send: send)
    }
  }

  /// The screenshot shown large over the whole window, if the Screenshots tab shows one.
  private var largeShot: ViewerLargeShot? {
    guard case .screenshots(let shots) = state.pane else { return nil }
    return shots.large
  }

  /// VoiceOver says what changed.
  ///
  /// - Parameter text: What to say.
  private func announce(_ text: String) {
    AccessibilityNotification.Announcement(text)
      .post()
  }
}

extension ViewerView where Input == EmptyView {
  /// Creates the window's content with no input surface and the state's own picture age, as previews and design
  /// renders draw it.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - send: Receives what the person does.
  public init(state: ViewerState, send: @escaping @MainActor (ViewerEvent) -> Void) {
    self.init(state: state, send: send, newest: { state.details?.newest ?? "" }, input: { EmptyView() })
  }
}

#if DEBUG
  #Preview("Working") {
    ViewerView(state: PreviewStates.viewerWorking) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("You have control") {
    ViewerView(state: PreviewStates.viewerControl) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Details") {
    ViewerView(state: PreviewStates.viewerDetails) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Needs you") {
    ViewerView(state: PreviewStates.viewerNeedsYou) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Activity") {
    ViewerView(state: PreviewStates.viewerActivity) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Apps") {
    ViewerView(state: PreviewStates.viewerApps) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Show on screen") {
    ViewerView(state: PreviewStates.viewerShowOnScreen) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Screenshots") {
    ViewerView(state: PreviewStates.viewerShots) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("A screenshot, large") {
    ViewerView(state: PreviewStates.viewerShotLarge) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.viewerWorking
    InteractivePreview {
      Menu(String("State")) {
        ForEach(PreviewStates.viewer) { preview in
          Button(preview.name) { state = preview.value }
        }
      }
      .fixedSize()
    } content: {
      ViewerView(state: state) { event in
        state = PreviewStates.viewer(state, applying: event)
      }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
    }
  }
#endif
