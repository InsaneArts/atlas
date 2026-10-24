import SwiftUI

/// The main window: one quiet bar at the top, the helper's screen under it, the Details column beside it on request,
/// a toast when Take Control is refused, and an alert over everything when one is asked. It draws its state and sends
/// what the person does; it decides nothing. The shell supplies `input`, the surface over the screen that takes the
/// mouse and keyboard while in control, and `livePicture`, which draws the newest picture through a layer (v3 V17), so
/// this view never sees AppKit; previews pass no input and an image.
public struct ViewerView<Input: View, LivePicture: View>: View {
  /// What the window shows.
  private let state: ViewerState
  /// Receives what the person does.
  private let send: @MainActor (ViewerEvent) -> Void
  /// Reads how old the newest picture is, as Details' row draws.
  private let newest: @MainActor () -> String
  /// The surface laid over the picture that takes the mouse and keyboard.
  private let input: Input
  /// Draws the newest picture.
  private let livePicture: (ViewerLivePicture) -> LivePicture
  /// The tab the stage last settled on, which the next switch goes from.
  @State private var settledTab: ViewerTab
  /// Which way the last switch went, from ``ViewerTab/direction(from:to:)``.
  @State private var settledDirection = 0
  /// The namespace a screenshot zooms in between its tile and the large view.
  @Namespace private var zoom
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates the window's content.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - send: Receives what the person does.
  ///   - newest: Reads how old the newest picture is as Details' row draws: the app's reads its controller, whose
  ///     clock ticks every second, so the picture's age redraws that row alone and not the window.
  ///   - input: The surface laid over the picture that takes the mouse and keyboard.
  ///   - livePicture: Draws the newest picture.
  public init(
    state: ViewerState,
    send: @escaping @MainActor (ViewerEvent) -> Void,
    newest: @escaping @MainActor () -> String,
    @ViewBuilder input: () -> Input,
    @ViewBuilder livePicture: @escaping (ViewerLivePicture) -> LivePicture
  ) {
    self.state = state
    self.send = send
    self.newest = newest
    self.input = input()
    self.livePicture = livePicture
    _settledTab = State(initialValue: state.pane.tab)
  }

  /// The top row in the title bar, the stage under it with Details beside it, a screenshot shown large over all of
  /// it, and the alert over that.
  public var body: some View {
    GeometryReader { proxy in
      VStack(spacing: .zero) {
        // The top row fills the title bar, whatever its height.
        ViewerTopRow(state: state.top, send: send)
          .frame(height: max(proxy.safeAreaInsets.top, ViewerLayout.topRowHeight))
        // The stage gives Details its room at once, never on an animation, so the picture takes its new width in one
        // step while Details slides in or out over the edge (v3 V3).
        stage
          .padding(.trailing, ViewerLayout.stageTrailing(showsDetails: showsDetails))
          .transaction(value: showsDetails) { $0.animation = nil }
          .overlay(alignment: .trailing) {
            ViewerInspector(details: state.details, newest: newest, send: send)
          }
      }
      // The window blurs behind a screenshot shown large, as its shade fades in (v3 V10).
      .blur(radius: ViewerLayout.shotBlur(isShown: largeShot != nil))
      .animation(ViewerLayout.shotFade, value: largeShot == nil)
      .overlay {
        LargeShotLayer(shot: largeShot, zoom: zoom, send: send)
          .allowsHitTesting(largeShot != nil)
      }
      .overlay {
        AlertLayer(state: state.alert) { send($0 == .confirm ? .alertConfirm : .alertCancel) }
      }
      .ignoresSafeArea(.container, edges: .top)
    }
    .windowSurface()
    .onChange(of: state.top.status.label) { _, label in announce(label) }
    .onChange(of: state.screen.notice) { _, notice in if let notice { announce(notice) } }
    .onChange(of: state.alert?.shownTitle) { _, title in if let title { announce(title) } }
    .onChange(of: state.alert?.refusal) { _, refusal in if let refusal { announce(refusal) } }
    .onChange(of: state.toast) { _, toast in if let toast { announce(toast) } }
    .onChange(of: state.pane.tab) { old, new in settle(from: old, to: new) }
  }

  /// The stage: the Screen tab or the selected tab's pane, in the frame every tab shares, the window's inset at its
  /// sides and below and none above, so the picture starts right under the bar (v3 `.vpane`); the small screen in its
  /// corner and the toast at its top.
  private var stage: some View {
    ZStack {
      pane
        .id(state.pane.tab)
        .transition(PaneShift.transition(reduceMotion: reduceMotion))
    }
    .environment(\.paneDirection, paneDirection)
    .padding([.horizontal, .bottom], Spacing.m)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    // The small screen keeps to the stage's corner, clear of Details.
    .overlay(alignment: .bottomTrailing) {
      if let mini = state.mini {
        MiniScreen(state: mini) { send(.selectTab(.screen)) }
          .padding(ViewerLayout.miniInset)
      }
    }
    .overlay(alignment: .top) { toast }
  }

  /// Whether Details shows.
  private var showsDetails: Bool {
    state.details != nil
  }

  /// Which way the stage moves for the tab it shows now: from the tab it last settled on as the switch begins, so
  /// the pane leaving and the pane arriving read the same side in the same moment, and that side once settled.
  private var paneDirection: Int {
    let tab = state.pane.tab
    return tab == settledTab ? settledDirection : ViewerTab.direction(from: settledTab, to: tab)
  }

  /// The Screen tab, or the selected tab's pane: Activity, Apps and Screenshots fill the frame inside the window's
  /// inset, and the Screen's picture fits within that same frame at its own shape.
  @ViewBuilder private var pane: some View {
    switch state.pane {
    case .screen:
      ViewerScreen(state: state.screen, send: send, input: input, livePicture: livePicture)
    case .activity(let activity):
      ViewerActivityPane(state: activity, send: send)
    case .apps(let apps):
      ViewerAppsPane(state: apps, send: send)
    case .screenshots(let shots):
      ViewerShotsPane(state: shots, zoom: zoom, send: send)
    }
  }

  /// Why Take Control was just refused, at the top of the stage: it arrives on a small spring and leaves by itself;
  /// under Reduce Motion it fades (v3 V12).
  private var toast: some View {
    ZStack {
      if let toast = state.toast {
        ViewerToastChip(text: toast)
          .padding(.top, Spacing.m)
          .transition(
            Motion.transition(
              AnyTransition.move(edge: .top)
                .combined(with: .opacity),
              reduceMotion: reduceMotion
            )
          )
      }
    }
    // In on a small spring, out quickly, as an action's slot comes and goes; a fade under Reduce Motion.
    .animation(ActionSlot.animation(isShown: state.toast != nil, reduceMotion: reduceMotion), value: state.toast)
    .allowsHitTesting(false)
  }

  /// The screenshot shown large over the whole window, if the Screenshots tab shows one.
  private var largeShot: ViewerLargeShot? {
    guard case .screenshots(let shots) = state.pane else { return nil }
    return shots.large
  }

  /// Settles the stage on a new tab once its switch has begun, keeping the way it went.
  ///
  /// - Parameters:
  ///   - old: The tab left.
  ///   - new: The tab shown.
  private func settle(from old: ViewerTab, to new: ViewerTab) {
    settledDirection = ViewerTab.direction(from: old, to: new)
    settledTab = new
  }

  /// VoiceOver says what changed.
  ///
  /// - Parameter text: What to say.
  private func announce(_ text: String) {
    AccessibilityNotification.Announcement(text)
      .post()
  }
}

extension ViewerView where Input == EmptyView, LivePicture == StillPicture {
  /// Creates the window's content with no input surface, the picture as an image and the state's own picture age, as
  /// previews and design renders draw it.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - send: Receives what the person does.
  public init(state: ViewerState, send: @escaping @MainActor (ViewerEvent) -> Void) {
    self.init(
      state: state,
      send: send,
      newest: { state.details?.newest ?? "" },
      input: { EmptyView() },
      livePicture: StillPicture.init
    )
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
