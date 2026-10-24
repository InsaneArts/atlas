import SwiftUI

/// One line of the Activity pane (v3 V8): its time, its kind's symbol in a tile tinted by how it went, what was done
/// and why it failed, and "now" while it runs or the screenshot it took with Open. The live line's tile has a blue
/// ring that cools a second after it arrives. It reads as one, with Open as its action.
struct ActivityLineRow: View {
  /// The line.
  let line: ViewerActivityLine
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The line as a timeline row, read as one.
  var body: some View {
    TimelineRow(time: line.time, title: line.text, reason: line.reason) {
      ActivityLineMark(line: line)
    } trailing: {
      trailing
    }
    .accessibilityElement(children: .combine)
    .accessibilityLabel(line.label)
    .accessibilityActions {
      if let shot = line.shot {
        Button("Open") { send(.openShot(shot.name)) }
      }
    }
  }

  /// "now" while it runs, the screenshot with Open for a screenshot's line, else nothing.
  @ViewBuilder private var trailing: some View {
    if let shot = line.shot {
      Button {
        send(.openShot(shot.name))
      } label: {
        HStack(spacing: Spacing.sPlus) {
          PictureTile(picture: shot.thumbnail, radius: Layout.thumbnailRadius)
            .frame(width: ViewerLayout.lineThumbnail.width, height: ViewerLayout.lineThumbnail.height)
          Text(verbatim: "Open")
        }
      }
      .buttonStyle(.textLink)
      .help("Show this screenshot large")
      .onAppear { send(.thumbnailNeeded(shot.name)) }
    } else if line.lead == .live {
      Text(verbatim: "now")
    }
  }
}

/// A line's mark: its kind's symbol in a small tile, quiet for an agent's, blue for the person's own, orange for a
/// problem; the live line's tile ringed in blue until the ring cools away, a second after it arrives (v3 V8, X10).
private struct ActivityLineMark: View {
  /// The line.
  let line: ViewerActivityLine
  /// Whether the live line's ring has cooled away; it does once per line, a second after the line arrives live.
  @State private var isCooled = false
  @Environment(\.animatesArrivals) private var animatesArrivals
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    SymbolTile(icon: .symbol(line.kind.symbol.rawValue), size: .small, tone: tone)
      .overlay {
        RoundedRectangle(cornerRadius: Layout.ringRadius)
          .strokeBorder(Color.accent, lineWidth: Stroke.emphasis)
          .padding(-Layout.ringReach)
          .opacity(showsRing ? Layout.ringOpacity : 0)
      }
      .onAppear(perform: cool)
      .onChange(of: line.lead) { cool() }
  }

  /// The tile's tone, by how the line went and who acted.
  private var tone: SymbolTile.Tone {
    switch line.lead {
    case .live, .quiet: .quiet
    case .person: .accent
    case .failed: .attention
    }
  }

  /// Whether the blue ring shows: on the live line until it cools.
  private var showsRing: Bool {
    line.lead == .live && !isCooled
  }

  /// Cools the live line's ring away after ``ViewerLayout/liveHold``, through the animation's own delay, never a
  /// sleep; renders keep it, as v3's card draws it.
  private func cool() {
    guard line.lead == .live, animatesArrivals else { return }
    let cooling = Motion.animation(Motion.state, reduceMotion: reduceMotion, reduced: Motion.reduced)
      .delay(ViewerLayout.liveHold)
    withAnimation(cooling) { isCooled = true }
  }
}

/// The line's own measures, from v3's Activity row (`.arow`).
private enum Layout {
  /// 5: the screenshot's thumbnail corner (v3 `.thumbm`).
  static let thumbnailRadius: CGFloat = 5
  /// 8: the live ring's corner, the tile's 6 and its 3-point reach, less a little (v3 `.arow.live .ic::after`).
  static let ringRadius: CGFloat = 8
  /// 3: how far the live ring stands outside the tile.
  static let ringReach: CGFloat = 3
  /// 0.9: the live ring's opacity before it cools.
  static let ringOpacity = 0.9
}

#if DEBUG
  #Preview("Lines") {
    if case .activity(let activity) = PreviewStates.viewerActivity.pane {
      VStack(spacing: .zero) {
        ForEach(activity.groups.flatMap(\.lines)) { line in
          ActivityLineRow(line: line) { _ in }
        }
      }
      .frame(width: SampleWidth.column * 2)
      .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var attempt = 0
    InteractivePreview {
      Button(String("Arrive again")) { attempt += 1 }
    } content: {
      if case .activity(let activity) = PreviewStates.viewerActivity.pane, let line = activity.groups.first?.lines.first
      {
        ActivityLineRow(line: line) { _ in }
          .id(attempt)
          .frame(width: SampleWidth.column * 2)
      }
    }
  }
#endif
