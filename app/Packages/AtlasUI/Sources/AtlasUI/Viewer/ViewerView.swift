import SwiftUI

/// The main window: one quiet bar at the top, the helper's screen under it, the Details column beside it on request,
/// a toast when Take Control is refused, and an alert over everything when one is asked. It draws its state and sends
/// what the person does; it decides nothing. The shell supplies `input`, the surface over the screen that takes the
/// mouse and keyboard while in control, and `livePicture`, which draws the newest picture through a layer, so this
/// view never sees AppKit; previews pass no input and an image.
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
  /// Whether the last switch moved the picture between the stage and the corner, from ``ScreenZoom``.
  @State private var settledZoom = false
  /// Activity, Apps and Screenshots as each last showed, kept built once shown, so a switch back fades the pane
  /// already there instead of building it again (``KeptPaneShift``).
  @State private var kept: [ViewerTab: ViewerPane] = [:]
  /// The corner the small screen keeps to: where the person last let go of it, as the window remembers it.
  @State private var miniCorner: MiniCorner
  /// How far the small screen is being dragged from its corner; zero while it rests.
  @State private var miniDrag = CGSize.zero
  /// The stage's size, which the small screen's size and corners are reckoned in.
  @State private var stageSize = CGSize.zero
  /// The namespace a screenshot zooms in between its tile and the large view.
  @Namespace private var zoom
  /// The namespace the helper's screen moves in between the stage and the corner.
  @Namespace private var screenZoom
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
    _miniCorner = State(initialValue: state.miniCorner)
  }

  /// The top row in the title bar, the stage under it with Details beside it or, in a narrow window, floating over
  /// its edge, a screenshot shown large over all of it, and the alert over that.
  public var body: some View {
    GeometryReader { proxy in
      let floats = ViewerLayout.detailsFloats(windowWidth: proxy.size.width)
      VStack(spacing: .zero) {
        // The top row fills the title bar, whatever its height.
        ViewerTopRow(state: state.top, send: send)
          .frame(height: max(proxy.safeAreaInsets.top, ViewerLayout.topRowHeight))
        // On the Screen the stage gives Details its room on Details' own slide, so the picture shrinks and recentres
        // in step with the column coming in, and turns back with it; the other tabs, and Reduce Motion, take it at once
        // as Details fades in over them.
        stage
          .padding(.trailing, ViewerLayout.stageTrailing(showsDetails: showsDetails, floats: floats))
          .animation(ViewerLayout.stageSlide(tab: state.pane.tab, reduceMotion: reduceMotion), value: showsDetails)
          .overlay(alignment: .trailing) {
            ViewerInspector(
              details: state.details,
              slides: ViewerLayout.detailsSlides(tab: state.pane.tab, reduceMotion: reduceMotion),
              floats: floats,
              newest: newest,
              send: send
            )
          }
      }
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
    .onChange(of: state.announcement) { _, text in if let text { announce(text) } }
    .onChange(of: state.screen.notice) { _, notice in if let notice { announce(notice) } }
    .onChange(of: state.alert?.shownTitle) { _, title in if let title { announce(title) } }
    .onChange(of: state.alert?.refusal) { _, refusal in if let refusal { announce(refusal) } }
    .onChange(of: state.toast) { _, toast in if let toast { announce(toast) } }
    .onChange(of: state.pane.tab) { old, new in settle(from: old, to: new) }
    .onChange(of: state.pane, initial: true) { _, pane in keep(pane) }
    .onChange(of: state.miniCorner) { _, corner in miniCorner = corner }
  }

  /// The stage: the Screen tab or the selected tab's pane, in the frame every tab shares, the window's inset at its
  /// sides and below and none above, so the picture starts right under the bar; the small screen in its corner and the
  /// toast at its top. The Screen is built as it shows and goes as it is left, so its picture and the surface that
  /// takes input exist only while it shows; Activity, Apps and Screenshots are kept once shown and only fade as the tab
  /// changes, the new one over the old. A switch between the Screen and a tab with the small screen moves the picture
  /// between the two on ``Motion/zoom``, the one thing that travels; every other switch only crossfades.
  private var stage: some View {
    ZStack {
      if state.pane.tab == .screen {
        ViewerScreen(state: state.screen, send: send, input: input, livePicture: livePicture, zoom: screenZoom)
          .transition(PaneShift.transition(isScreen: true))
          .zIndex(1)
      }
      ForEach(ViewerTab.keptTabs, id: \.self) { tab in
        if let pane = pane(for: tab) {
          KeptPane(pane: pane, zoom: zoom, send: send)
            .modifier(KeptPaneShift(isShown: tab == state.pane.tab))
            .transition(
              .asymmetric(
                insertion: PaneShift.transition(isScreen: false),
                removal: .identity
              )
            )
        }
      }
    }
    .environment(\.miniClearance, miniClearance)
    .padding([.horizontal, .bottom], Spacing.m)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    // The small screen keeps to a corner of the stage, clear of Details.
    .overlay { corner }
    .overlay(alignment: .top) { toast }
    .environment(\.screenZooms, screenZooms)
    .animation(ScreenZoom.animation(zooms: screenZooms), value: state.pane.tab)
  }

  /// The helper's screen, small and live in a corner of the stage over Activity and Apps: the same live slot as the
  /// stage's picture, which it grows out of as the Screen is left and turns back into as it is chosen again, in
  /// whichever corner it keeps to, sized to the stage. The person can drag it anywhere, or throw it; let go, it lands
  /// in the corner its momentum heads for, at the speed it was thrown (at once under Reduce Motion), and a click with
  /// no drag still goes back to the Screen.
  private var corner: some View {
    let size = ViewerLayout.miniSize(stageWidth: stageSize.width)
    return ZStack {
      if let mini = state.mini {
        MiniScreen(state: mini) {
          send(.selectTab(.screen))
        } picture: {
          livePicture(
            ViewerLivePicture(
              picture: mini.picture,
              saturation: PictureTreatment.saturation(for: mini.look),
              corner: Radius.medium
            )
          )
        }
        .screenZoom(in: screenZoom)
        .frame(width: size.width, height: size.height)
        .offset(miniDrag)
        .highPriorityGesture(miniDragging)
        .padding(ViewerLayout.miniInset)
        .transition(CornerShift.transition(growsFromStage: zoomingNow, reduceMotion: reduceMotion))
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: miniCorner.alignment)
    .onGeometryChange(for: CGSize.self) { proxy in
      proxy.size
    } action: { size in
      stageSize = size
    }
  }

  /// Dragging the small screen: it follows the pointer freely, then lands in the corner nearest where its momentum
  /// would carry it, going on from where it was let go at the speed it was going, as its corner and its drag change
  /// together; the window remembers the corner.
  private var miniDragging: some Gesture {
    DragGesture(minimumDistance: ViewerLayout.miniDragStart, coordinateSpace: .global)
      .onChanged { drag in
        miniDrag = drag.translation
      }
      .onEnded { drag in
        let landing = miniCorner.settling(after: drag.predictedEndTranslation, in: stageSize)
        let velocity = miniCorner.landingVelocity(
          to: landing,
          after: drag.translation,
          velocity: drag.velocity,
          in: stageSize
        )
        withAnimation(ViewerLayout.miniLanding(velocity: velocity, reduceMotion: reduceMotion)) {
          miniCorner = landing
          miniDrag = .zero
        }
        if landing != state.miniCorner { send(.moveCorner(landing)) }
      }
  }

  /// How far the panes' lists keep clear of the small screen: at the end of its corner while it shows, else not at all.
  private var miniClearance: MiniClearance {
    guard state.mini != nil else { return MiniClearance() }
    return MiniClearance(
      edges: miniCorner.isTop ? .top : .bottom,
      length: ViewerLayout.miniRoom(stageWidth: stageSize.width)
    )
  }

  /// Whether Details shows.
  private var showsDetails: Bool {
    state.details != nil
  }

  /// Whether the picture moves between the stage and the corner for the tab shown now: from the tab last settled on
  /// as the switch begins, and that answer once settled, so the Screen's pane leaving reads the same answer from its
  /// first frame to its last.
  private var screenZooms: Bool {
    let tab = state.pane.tab
    guard tab != settledTab else { return settledZoom }
    return ScreenZoom.zooms(from: settledTab, to: tab, screen: state.screen, reduceMotion: reduceMotion)
  }

  /// Whether the picture is moving between the stage and the corner in this very switch, before it settles: what the
  /// small screen arriving reads, so a small screen that comes with the first picture, with no switch, fades in.
  private var zoomingNow: Bool {
    state.pane.tab != settledTab && screenZooms
  }

  /// A kept tab's pane as it shows now, or as it last showed while another tab shows; nil until it has shown.
  ///
  /// - Parameter tab: Activity, Apps or Screenshots.
  /// - Returns: Its pane, or nil.
  private func pane(for tab: ViewerTab) -> ViewerPane? {
    state.pane.tab == tab ? state.pane : kept[tab]
  }

  /// Why Take Control was just refused, at the top of the stage: it comes down a little as it fades in and fades out
  /// by itself; under Reduce Motion it only fades (``ViewerToastChip/transition(reduceMotion:)``).
  private var toast: some View {
    ZStack {
      if let toast = state.toast {
        ViewerToastChip(text: toast)
          .padding(.top, Spacing.m)
          .transition(ViewerToastChip.transition(reduceMotion: reduceMotion))
      }
    }
    .allowsHitTesting(false)
  }

  /// The screenshot shown large over the whole window, if the Screenshots tab shows one.
  private var largeShot: ViewerLargeShot? {
    guard case .screenshots(let shots) = state.pane else { return nil }
    return shots.large
  }

  /// Settles the stage on a new tab once its switch has begun, keeping whether the picture moved.
  ///
  /// - Parameters:
  ///   - old: The tab left.
  ///   - new: The tab shown.
  private func settle(from old: ViewerTab, to new: ViewerTab) {
    settledZoom = ScreenZoom.zooms(from: old, to: new, screen: state.screen, reduceMotion: reduceMotion)
    settledTab = new
  }

  /// Keeps Activity, Apps or Screenshots as it shows now, to draw while another tab shows.
  ///
  /// - Parameter pane: The pane shown.
  private func keep(_ pane: ViewerPane) {
    guard pane.tab.isKept, kept[pane.tab] != pane else { return }
    kept[pane.tab] = pane
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
