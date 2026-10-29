import SwiftUI

/// The main window: the sidebar of sessions, the Library and the helper's health at its leading edge, shown or hidden,
/// and beside it one solid content card with a slim header saying who has the helper, the stage, and a footer with the
/// latest action and the controls; and an alert over everything when one is asked. It draws its state and sends what
/// the person does; it decides nothing. The shell supplies `input`, the surface over the screen that takes the mouse
/// and keyboard while in control, and `livePicture`, which draws the newest picture through a layer, so this view
/// never sees AppKit; previews pass no input and an image.
public struct ViewerView<Input: View, LivePicture: View>: View {
  /// What the window shows.
  private let state: ViewerState
  /// Receives what the person does.
  private let send: @MainActor (ViewerEvent) -> Void
  /// The surface laid over the picture that takes the mouse and keyboard.
  private let input: Input
  /// Draws the newest picture.
  private let livePicture: (ViewerLivePicture) -> LivePicture
  /// The place the stage last settled on, which the next switch goes from.
  @State private var settledPlace: ViewerPlace
  /// Whether the last switch moved the picture between the stage and the corner, from ``ScreenZoom``.
  @State private var settledZoom = false
  /// Activity, Screenshots and Apps as each last showed, kept built once shown, so a switch back fades the pane
  /// already there instead of building it again (``KeptPaneShift``).
  @State private var kept: [ViewerPlace: ViewerPane] = [:]
  /// Sequences list departures and arrivals, cancelling superseded destinations.
  @State private var paneHandoff: PaneHandoff
  /// The corner the small screen keeps to: where the person last let go of it, as the window remembers it.
  @State private var miniCorner: MiniCorner
  /// How far the small screen is being dragged from its corner; zero while it rests.
  @State private var miniDrag = CGSize.zero
  /// The stage's size, which the small screen's corners are reckoned in, written with no animation so a resize or the
  /// sidebar's slide never animates it.
  @State private var stageSize = CGSize.zero
  /// Whether the picture rests in the corner, small, rather than on the stage: what its corners round to, the small
  /// screen's or the stage's, changing on the picture's own spring as it moves between the two.
  @State private var pictureInCorner: Bool
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
  ///   - input: The surface laid over the picture that takes the mouse and keyboard.
  ///   - livePicture: Draws the newest picture.
  public init(
    state: ViewerState,
    send: @escaping @MainActor (ViewerEvent) -> Void,
    @ViewBuilder input: () -> Input,
    @ViewBuilder livePicture: @escaping (ViewerLivePicture) -> LivePicture
  ) {
    self.state = state
    self.send = send
    self.input = input()
    self.livePicture = livePicture
    _paneHandoff = State(initialValue: PaneHandoff(state.pane.place))
    _settledPlace = State(initialValue: state.pane.place)
    _miniCorner = State(initialValue: state.miniCorner)
    _pictureInCorner = State(initialValue: state.pane.place.keepsScreenInCorner)
  }

  /// The sidebar beside the card, the toggle over them in the window buttons' band; a screenshot shown large over all
  /// of it, and the alert over that.
  public var body: some View {
    GeometryReader { proxy in
      SidebarSplit(sidebar: state.sidebar, isFullScreen: state.isFullScreen, send: send) { slide in
        card(slide: slide, listWidth: listWidth(windowWidth: proxy.size.width, settling: slide.settling))
          .padding(.vertical, ViewerLayout.cardInset)
          .padding(.trailing, ViewerLayout.cardInset)
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
    .background(Color.groundRecessed)
    .foregroundStyle(Color.textPrimary)
    // A resize holds every highlight still, so nothing flickers as rows and buttons pass under a still pointer.
    .environment(\.hoverPaused, state.isResizing)
    .onChange(of: state.announcement) { _, text in if let text { announce(text) } }
    .onChange(of: state.footer.latest.id) { _, _ in
      if state.footer.latest.announces { announce(state.footer.latest.words) }
    }
    .onChange(of: state.alert?.shownTitle) { _, title in if let title { announce(title) } }
    .onChange(of: state.alert?.refusal) { _, refusal in if let refusal { announce(refusal) } }
    .onChange(of: state.pane.place) { old, new in settle(from: old, to: new) }
    .onChange(of: state.pane, initial: true) { _, pane in keep(pane) }
    .onChange(of: state.miniCorner) { _, corner in miniCorner = corner }
  }

  /// The content card: who has the helper over the stage, and the latest action with the controls under it. The
  /// header's and the footer's words keep the room they had while the card's edge travels (``SidebarSlide``).
  ///
  /// - Parameters:
  ///   - slide: How the sidebar stands as it moves: the header's words keep clear of it as drawn now.
  ///   - listWidth: The width the lists are laid out at.
  /// - Returns: The card.
  private func card(slide: SidebarSlide, listWidth: CGFloat) -> some View {
    let isFullScreen = state.isFullScreen
    return ViewerCard(
      header: state.header,
      headerLeading: ViewerLayout.headerLeading(mode: slide.drawn, isFullScreen: isFullScreen),
      headerHold: slide.wordsHold { ViewerLayout.headerLeading(mode: $0, isFullScreen: isFullScreen) },
      footer: state.footer,
      footerHold: slide.wordsHold { _ in .zero },
      send: send
    ) {
      stage(listWidth: listWidth)
    }
  }

  /// The width Activity, Screenshots and Apps are laid out at, the whole card's: as it stands once the sidebar has
  /// settled, so a list is laid out once as the sidebar hides or shows, pinned to the card's trailing edge, which does
  /// not move, and uncovered or covered as the card's leading edge travels rather than laid out again at every
  /// frame. The small screen is sized from it too, so it keeps its size through the slide.
  ///
  /// - Parameters:
  ///   - windowWidth: The window's width.
  ///   - settling: The sidebar's mode once settled.
  /// - Returns: The lists' width.
  private func listWidth(windowWidth: CGFloat, settling: ViewerSidebarMode) -> CGFloat {
    ViewerLayout.cardWidth(windowWidth: windowWidth, mode: settling)
  }

  /// The stage: the Screen or the chosen list's pane in the card's body, with the small screen in its corner over a
  /// list. The Screen sits inside the stage's inset; the lists reach the card's edges, which frame them, as their rows
  /// keep their own insets. The Screen is built as it shows and goes as it is left, so its picture and the surface that
  /// takes input exist only while it shows; Activity, Screenshots and Apps are kept once shown and only fade as the
  /// place changes, the new one over the old, rising in from the Screen and sinking back to it (``ListRise``). A switch
  /// between the Screen and a list moves the picture between the two on ``Motion/zoom``, the one thing that travels,
  /// its corners rounding with it; every other switch only crossfades.
  ///
  /// - Parameter listWidth: The width the lists are laid out at, pinned to the stage's trailing edge.
  /// - Returns: The stage.
  private func stage(listWidth: CGFloat) -> some View {
    let pip = ViewerLayout.pipSize(bodyWidth: listWidth, aspect: state.mini?.picture.aspectRatio)
    return ZStack {
      if state.pane.place == .screen {
        ViewerScreen(
          state: state.screen,
          send: send,
          input: input,
          livePicture: livePicture,
          zoom: screenZoom,
          corner: pictureCorner,
          zooms: screenZooms
        )
        .padding(ViewerLayout.stageInset)
        .transition(PaneShift.transition(isScreen: true))
        .zIndex(1)
      }
      ForEach(ViewerPlace.keptPlaces, id: \.self) { place in
        if let pane = pane(for: place) {
          KeptPane(pane: pane, zoom: zoom, send: send)
            .frame(width: listWidth)
            .frame(minWidth: .zero, maxWidth: .infinity, alignment: .trailing)
            .modifier(
              KeptPaneShift(isShown: paneHandoff.shown == place, rises: listRises) {
                forgetPictures(of: place)
                paneHandoff.departed(place)
              }
            )
        }
      }
    }
    .onChange(of: state.pane.place) { _, place in paneHandoff.request(place) }
    .environment(\.miniClearance, miniClearance(pip))
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .clipped()
    .overlay { corner(pip) }
    .environment(\.screenZooms, screenZooms)
    .animation(ScreenZoom.animation(zooms: screenZooms), value: state.pane.place)
  }

  /// The helper's screen, small and live in a corner of the card's body over every list, clear of the footer: the same
  /// live slot as the stage's picture, which it grows out of as the Screen is left and turns back into as it is chosen
  /// again, in whichever corner it keeps to. The person can drag it anywhere, or throw it; let go, it lands in the
  /// corner its momentum heads for, at the speed it was thrown (at once under Reduce Motion), and a click with no drag
  /// still goes back to the Screen.
  ///
  /// - Parameter size: The small screen's size.
  /// - Returns: The corner.
  private func corner(_ size: CGSize) -> some View {
    ZStack {
      if let mini = state.mini {
        MiniScreen(state: mini, corner: pictureCorner) {
          send(.show(.screen))
        } picture: {
          livePicture(
            ViewerLivePicture(
              picture: mini.picture,
              saturation: PictureTreatment.saturation(for: mini.look),
              corner: pictureCorner,
              zooms: screenZooms
            )
          )
        }
        .screenZoom(in: screenZoom)
        .frame(width: size.width, height: size.height)
        .offset(miniDrag)
        .highPriorityGesture(miniDragging(size))
        .padding(ViewerLayout.pipInset)
        .transition(CornerShift.transition(growsFromStage: zoomingNow, reduceMotion: reduceMotion))
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: miniCorner.alignment)
    .onGeometryChange(for: CGSize.self) { proxy in
      proxy.size
    } action: { size in
      var still = Transaction(animation: nil)
      still.disablesAnimations = true
      withTransaction(still) { stageSize = size }
    }
  }

  /// Dragging the small screen: it follows the pointer freely, then lands in the corner nearest where its momentum
  /// would carry it, going on from where it was let go at the speed it was going, as its corner and its drag change
  /// together; the window remembers the corner.
  ///
  /// - Parameter size: The small screen's size.
  /// - Returns: The gesture.
  private func miniDragging(_ size: CGSize) -> some Gesture {
    DragGesture(minimumDistance: ViewerLayout.miniDragStart, coordinateSpace: .global)
      .onChanged { drag in
        miniDrag = drag.translation
      }
      .onEnded { drag in
        let landing = miniCorner.settling(after: drag.predictedEndTranslation, in: stageSize, mini: size)
        let velocity = miniCorner.landingVelocity(
          to: landing,
          after: drag.translation,
          velocity: drag.velocity,
          in: stageSize,
          mini: size
        )
        withAnimation(ViewerLayout.miniLanding(velocity: velocity, reduceMotion: reduceMotion)) {
          miniCorner = landing
          miniDrag = .zero
        }
        if landing != state.miniCorner { send(.moveCorner(landing)) }
      }
  }

  /// Where the small screen floats over the panes' lists: the end of the corner it is in now and the band its size
  /// takes there while it shows, else nowhere.
  ///
  /// - Parameter pip: The small screen's size.
  /// - Returns: The clearance.
  private func miniClearance(_ pip: CGSize) -> MiniClearance {
    guard state.mini != nil else { return MiniClearance() }
    return MiniClearance(edges: miniCorner.isTop ? .top : .bottom, length: ViewerLayout.pipRoom(pip))
  }

  /// The picture's corners, on the stage or in the corner: the small screen's while it rests there, the stage's while
  /// it rests on the stage, and between the two as it moves.
  private var pictureCorner: CGFloat {
    pictureInCorner ? ViewerLayout.pipRadius : ViewerLayout.screenRadius
  }

  /// Whether the switch under way moves the lists in depth, from or to the Screen, so the list shown rises in or the
  /// one left sinks out: from the place last settled on, which a list's fade reads as the switch begins.
  private var listRises: Bool {
    ListRise.rises(from: settledPlace, to: state.pane.place, reduceMotion: reduceMotion)
  }

  /// Whether the picture moves between the stage and the corner for the place shown now: from the place last settled on
  /// as the switch begins, and that answer once settled, so the Screen's pane leaving reads the same answer from its
  /// first frame to its last.
  private var screenZooms: Bool {
    let place = state.pane.place
    guard place != settledPlace else { return settledZoom }
    return ScreenZoom.zooms(from: settledPlace, to: place, screen: state.screen, reduceMotion: reduceMotion)
  }

  /// Whether the picture is moving between the stage and the corner in this very switch, before it settles: what the
  /// small screen arriving reads, so a small screen that comes with the first picture, with no switch, fades in.
  private var zoomingNow: Bool {
    state.pane.place != settledPlace && screenZooms
  }

  /// A kept place's pane as it shows now, or as it last showed while another place shows; nil until it has shown.
  ///
  /// - Parameter place: Activity, Apps or Screenshots.
  /// - Returns: Its pane, or nil.
  private func pane(for place: ViewerPlace) -> ViewerPane? {
    state.pane.place == place ? state.pane : kept[place]
  }

  /// The screenshot shown large over the whole window, if the Screenshots list shows one.
  private var largeShot: ViewerLargeShot? {
    guard case .screenshots(let shots) = state.pane else { return nil }
    return shots.large
  }

  /// Settles the stage on a new place once its switch has begun, keeping whether the picture moved.
  ///
  /// - Parameters:
  ///   - old: The place left.
  ///   - new: The place shown.
  private func settle(from old: ViewerPlace, to new: ViewerPlace) {
    let zooms = ScreenZoom.zooms(from: old, to: new, screen: state.screen, reduceMotion: reduceMotion)
    settledZoom = zooms
    settledPlace = new
    // The picture's corners round on its own spring as it moves, its layer's with it; at once when it does not move.
    let inCorner = new.keepsScreenInCorner
    guard inCorner != pictureInCorner else { return }
    withAnimation(ScreenZoom.animation(zooms: zooms)) { pictureInCorner = inCorner }
  }

  /// Keeps Activity, Screenshots or Apps as it shows now, to draw while another place shows.
  ///
  /// - Parameter pane: The pane shown.
  private func keep(_ pane: ViewerPane) {
    guard pane.place.isKept, kept[pane.place] != pane else { return }
    kept[pane.place] = pane
  }

  /// Forgets the pictures of the helper's screen a kept place drew, once it has faded out, so none outlives the place
  /// showing; it draws them anew from the next state as it shows again.
  ///
  /// - Parameter place: The place that faded out.
  private func forgetPictures(of place: ViewerPlace) {
    guard place != state.pane.place, let pane = kept[place], pane.forgettingPictures != pane else { return }
    kept[place] = pane.forgettingPictures
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
  /// Creates the window's content with no input surface and the picture as an image, as previews and design renders
  /// draw it.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - send: Receives what the person does.
  public init(state: ViewerState, send: @escaping @MainActor (ViewerEvent) -> Void) {
    self.init(
      state: state,
      send: send,
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

  #Preview("Stopped") {
    ViewerView(state: PreviewStates.viewerStopped) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Sessions") {
    ViewerView(state: PreviewStates.viewerSessionsEnded) { _ in }
      .frame(width: PreviewStates.viewerSize.width, height: PreviewStates.viewerSize.height)
  }

  #Preview("Activity, one session") {
    ViewerView(state: PreviewStates.viewerActivityOneSession) { _ in }
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

  #Preview("Motion") {
    ViewerMotionLab()
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
