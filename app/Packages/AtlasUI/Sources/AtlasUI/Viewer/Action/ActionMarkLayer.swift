import SwiftUI

/// The mark drawn over an action's picture, never into it: a click's ring and dots, a typing's ring and bubble, a key's
/// soft ring and keycaps, a scroll's pill and count, a drag's dot, dashed path and ring, a press's ring or its tag, and
/// for one that did not work, why. A ring goes around the element the action went to where its frame is known, and is
/// a fixed size centred on the spot where it is not. Compact, over a small picture, it draws only what reads at that
/// size: rings, dots, pills and paths.
struct ActionMarkLayer: View {
  /// The mark.
  let mark: ViewerActionMark
  /// How large it draws.
  var style = ActionMarkStyle.full
  /// Where the picture stands in this layer, which the mark's shares are measured against; the layer's own bounds when
  /// nil.
  var frame: CGRect?

  /// The mark's pieces, each at its place over the picture.
  var body: some View {
    GeometryReader { proxy in
      let picture = frame ?? CGRect(origin: .zero, size: proxy.size)
      ZStack(alignment: .topLeading) {
        pieces(in: picture)
      }
      .frame(width: proxy.size.width, height: proxy.size.height, alignment: .topLeading)
    }
    .allowsHitTesting(false)
    .accessibilityHidden(true)
  }

  /// The pieces of the mark's shape.
  ///
  /// - Parameter picture: Where the picture stands.
  /// - Returns: The pieces.
  @ViewBuilder private func pieces(in picture: CGRect) -> some View {
    let tone = mark.tone
    let spot = mark.spot.map { place($0, in: picture) }
    let box = ringBox(in: picture)
    switch mark.shape {
    case .click(let clicks):
      if let box {
        ring(box)
        if let spot { dots(clicks, at: spot) }
        note(beside: box, in: picture)
      }
    case .typing(let text, let detail):
      if let box {
        ring(box)
        if style == .full {
          hung(from: box, in: picture, room: picture.width * ViewerLayout.markBubbleShare) {
            MarkBubble(text: text, detail: detail, width: picture.width * ViewerLayout.markBubbleShare)
          }
        }
        note(beside: box, in: picture)
      } else if style == .full {
        MarkBubble(text: text, detail: detail, width: picture.width * ViewerLayout.markBubbleShare)
          .frame(width: picture.width * ViewerLayout.markBubbleShare)
          .position(x: picture.midX, y: picture.midY)
      }
    case .keys(let keys, let name):
      if let box {
        ring(box)
        if style == .full {
          hung(from: box, in: picture, room: picture.width) { MarkKeys(keys: keys, name: name, tone: tone) }
        }
        note(beside: box, in: picture)
      } else if style == .full {
        MarkKeys(keys: keys, name: name, tone: tone)
          .position(x: picture.midX, y: picture.midY)
      }
    case .scroll(let arrow, let label):
      if let spot {
        MarkScrollPill(arrow: arrow, tone: tone, style: style)
          .position(spot)
        if style == .full {
          beside(spot, reach: style.pill * ViewerLayout.markPillLength / 2 + ViewerLayout.markGap) {
            MarkTag(text: mark.note.map { "\(label) · \($0)" } ?? label, tone: tone)
          }
        }
      }
    case .drag(let to):
      if let spot {
        let end = place(to, in: picture)
        path(from: spot, to: end)
        MarkDot(tone: tone, style: style)
          .position(spot)
        MarkRing(tone: tone, style: style)
          .position(end)
        note(beside: square(at: end), in: picture)
      }
    case .press:
      if let box {
        ring(box)
        note(beside: box, in: picture)
      } else if style == .full {
        MarkTag(text: mark.note ?? "Pressed by name", tone: tone)
          .position(x: picture.midX, y: picture.midY)
      }
    }
  }

  /// What a ring goes around in this layer: the element's frame where it is known, never smaller than two dots
  /// across, else a ring's square on the spot; nil when neither is known.
  ///
  /// - Parameter picture: Where the picture stands.
  /// - Returns: The ring's frame, or nil.
  private func ringBox(in picture: CGRect) -> CGRect? {
    if let area = mark.area {
      let least = style.dot * 2
      let frame = CGRect(
        x: picture.minX + area.minX * picture.width,
        y: picture.minY + area.minY * picture.height,
        width: area.width * picture.width,
        height: area.height * picture.height
      )
      return frame.insetBy(dx: min(.zero, (frame.width - least) / 2), dy: min(.zero, (frame.height - least) / 2))
    }
    return mark.spot.map { square(at: place($0, in: picture)) }
  }

  /// A ring's square centred on a place.
  ///
  /// - Parameter center: The place.
  /// - Returns: The square.
  private func square(at center: CGPoint) -> CGRect {
    CGRect(x: center.x - style.ring / 2, y: center.y - style.ring / 2, width: style.ring, height: style.ring)
  }

  /// The ring: around the element where its frame is known, else a fixed size on the spot.
  ///
  /// - Parameter box: The ring's frame.
  /// - Returns: The ring.
  @ViewBuilder private func ring(_ box: CGRect) -> some View {
    if mark.area != nil {
      MarkOutline(tone: mark.tone, style: style)
        .frame(width: box.width, height: box.height)
        .position(x: box.midX, y: box.midY)
    } else {
      MarkRing(tone: mark.tone, style: style)
        .position(x: box.midX, y: box.midY)
    }
  }

  /// A spot's place in this layer.
  ///
  /// - Parameters:
  ///   - share: The spot, as a share of the picture's width and height.
  ///   - picture: Where the picture stands.
  /// - Returns: The place.
  private func place(_ share: CGPoint, in picture: CGRect) -> CGPoint {
    CGPoint(x: picture.minX + share.x * picture.width, y: picture.minY + share.y * picture.height)
  }

  /// One dot for a click, two side by side for a double click.
  ///
  /// - Parameters:
  ///   - clicks: How many times it clicked.
  ///   - spot: Where.
  /// - Returns: The dots.
  @ViewBuilder private func dots(_ clicks: Int, at spot: CGPoint) -> some View {
    if clicks > 1 {
      MarkDot(tone: mark.tone, style: style)
        .position(x: spot.x - style.dot * ViewerLayout.markDotSpread, y: spot.y)
      MarkDot(tone: mark.tone, style: style)
        .position(x: spot.x + style.dot * ViewerLayout.markDotSpread, y: spot.y)
    } else {
      MarkDot(tone: mark.tone, style: style)
        .position(spot)
    }
  }

  /// Why the action did not work, on a tag to the right of its ring over a large picture, or under it where the ring
  /// ends near the picture's right edge.
  ///
  /// - Parameters:
  ///   - box: The ring's frame.
  ///   - picture: Where the picture stands.
  /// - Returns: The tag, or nothing.
  @ViewBuilder private func note(beside box: CGRect, in picture: CGRect) -> some View {
    if style == .full, let note = mark.note {
      if box.maxX - picture.minX > picture.width * ViewerLayout.markNearRight {
        Color.clear
          .frame(width: .zero, height: .zero)
          .overlay(alignment: .topLeading) {
            MarkTag(text: note, tone: mark.tone)
              .fixedSize()
              .offset(y: ViewerLayout.markGap)
          }
          .position(x: max(box.minX, picture.minX), y: box.maxY)
      } else {
        beside(CGPoint(x: box.maxX, y: box.midY), reach: ViewerLayout.markGap) {
          MarkTag(text: note, tone: mark.tone)
        }
      }
    }
  }

  /// Content standing to the right of a spot, its leading edge `reach` past it and its middle level with it.
  ///
  /// - Parameters:
  ///   - spot: The spot.
  ///   - reach: How far past the spot it starts.
  ///   - content: What stands there.
  /// - Returns: The placed content.
  private func beside<Content: View>(_ spot: CGPoint, reach: CGFloat, @ViewBuilder content: () -> Content)
    -> some View
  {
    Color.clear
      .frame(width: .zero, height: .zero)
      .overlay(alignment: .leading) {
        content()
          .fixedSize()
          .offset(x: reach)
      }
      .position(spot)
  }

  /// A bubble or keys hung over the ring, clear of it: under it near the picture's top, and leaning inwards near
  /// either side, so it stays over the picture.
  ///
  /// - Parameters:
  ///   - box: The ring's frame.
  ///   - picture: Where the picture stands, which says how near an edge the ring is.
  ///   - room: The most width the content may take.
  ///   - content: The bubble or keys.
  /// - Returns: The hung content.
  private func hung<Content: View>(
    from box: CGRect,
    in picture: CGRect,
    room: CGFloat,
    @ViewBuilder content: () -> Content
  )
    -> some View
  {
    let share = CGPoint(
      x: picture.width > 0 ? (box.midX - picture.minX) / picture.width : 0.5,
      y: picture.height > 0 ? (box.midY - picture.minY) / picture.height : 0.5
    )
    let below = share.y < ViewerLayout.markNearTop
    let horizontal: HorizontalAlignment =
      share.x < ViewerLayout.markNearSide
      ? .leading : share.x > 1 - ViewerLayout.markNearSide ? .trailing : .center
    let anchor: CGFloat =
      switch horizontal {
      case .leading: box.minX
      case .trailing: box.maxX
      default: box.midX
      }
    return Color.clear
      .frame(width: .zero, height: .zero)
      .overlay(alignment: Alignment(horizontal: horizontal, vertical: below ? .top : .bottom)) {
        // Offered the picture's width, so a bubble wraps at its own widest rather than at the anchor's none.
        content()
          .frame(width: room, alignment: Alignment(horizontal: horizontal, vertical: .center))
          .offset(y: below ? ViewerLayout.markGap : -ViewerLayout.markGap)
      }
      .position(x: anchor, y: below ? box.maxY : box.minY)
  }

  /// A drag's dashed path, over a white halo.
  ///
  /// - Parameters:
  ///   - start: Where it started.
  ///   - end: Where it was dropped.
  /// - Returns: The path.
  private func path(from start: CGPoint, to end: CGPoint) -> some View {
    let line = Path { path in
      path.move(to: start)
      path.addLine(to: end)
    }
    return ZStack {
      line.stroke(
        Color.markHalo,
        style: StrokeStyle(lineWidth: style.line * ViewerLayout.markHaloWidth, lineCap: .round)
      )
      line.stroke(
        mark.tone.color,
        style: StrokeStyle(lineWidth: style.line, lineCap: .round, dash: ViewerLayout.markDash)
      )
    }
  }
}

#if DEBUG
  #Preview("Marks") {
    ScrollView {
      LazyVGrid(columns: [GridItem(.adaptive(minimum: SampleWidth.narrow))], spacing: Spacing.l) {
        ForEach(PreviewStates.sampleMarks, id: \.name) { sample in
          VStack(alignment: .leading, spacing: Spacing.xs) {
            MarkedPicture(picture: PreviewStates.shotPicture(PreviewStates.beforeNames[0]), mark: sample.mark)
              .aspectRatio(PreviewStates.shotAspect, contentMode: .fit)
            Text(verbatim: sample.name)
              .font(Typography.font(.callout))
          }
        }
      }
    }
    .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
    .sampleSurface()
  }

  #Preview("Marks, dark") {
    ScrollView {
      LazyVGrid(columns: [GridItem(.adaptive(minimum: SampleWidth.narrow))], spacing: Spacing.l) {
        ForEach(PreviewStates.sampleMarks, id: \.name) { sample in
          VStack(alignment: .leading, spacing: Spacing.xs) {
            MarkedPicture(picture: PreviewStates.shotPicture(PreviewStates.beforeNames[0]), mark: sample.mark)
              .aspectRatio(PreviewStates.shotAspect, contentMode: .fit)
            Text(verbatim: sample.name)
              .font(Typography.font(.callout))
          }
        }
      }
    }
    .frame(width: ViewerLayout.largeShotLargest.width, height: ViewerLayout.largeShotLargest.height)
    .sampleSurface()
    .environment(\.colorScheme, .dark)
  }
#endif
