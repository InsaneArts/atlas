import Foundation

/// The shared capsule's interaction state. Only release produces a pointer choice; sampling never does.
struct TabsMotion {
  /// The measured track.
  var geometry = TabsGeometry()
  /// The committed destination's index.
  private(set) var selected: Int?
  /// The resting or directly manipulated centre.
  private(set) var centre: CGFloat = .zero
  /// The current spring; a press captures its live handoff before taking ownership.
  private(set) var spring: Motion.SegmentSpring?
  /// Whether a pointer owns this interaction.
  private(set) var isPressed = false
  /// Whether the pointer has crossed the drag threshold.
  private(set) var isDragging = false
  /// The outgoing thumb in a reduced-motion crossfade.
  private(set) var outgoing: CGRect?
  /// When that crossfade began.
  private(set) var fadeStart: TimeInterval = .zero
  private var grabCentre: CGFloat = .zero
  private var startPointer: CGFloat = .zero
  private var rawCentre: CGFloat = .zero
  private var heldVelocity: CGFloat = .zero

  /// Whether the display clock can sleep.
  var isIdle: Bool { spring == nil && outgoing == nil }

  /// The live centre and velocity, including a spring interrupted between display frames.
  ///
  /// - Parameter time: The clock time.
  /// - Returns: The presentation position and velocity.
  func sample(at time: TimeInterval) -> (position: CGFloat, velocity: CGFloat) {
    if isPressed && !isDragging { return (centre, heldVelocity) }
    return spring?.sample(at: time) ?? (centre, .zero)
  }

  /// The thumb's current frame.
  ///
  /// - Parameter time: The clock time.
  /// - Returns: Its interpolated frame.
  func frame(at time: TimeInterval) -> CGRect { geometry.thumb(at: sample(at: time).position) }

  /// The incoming thumb's opacity during a reduced-motion crossfade.
  ///
  /// - Parameter time: The clock time.
  /// - Returns: A share from zero to one.
  func fade(at time: TimeInterval) -> Double {
    guard outgoing != nil else { return 1 }
    return min(max((time - fadeStart) / Motion.Duration.crossfadeReduced.seconds, .zero), 1)
  }

  /// The outgoing thumb's complementary crossfade opacity.
  ///
  /// - Parameter time: The clock time.
  /// - Returns: A share from one to zero.
  func outgoingFade(at time: TimeInterval) -> Double { 1 - fade(at: time) }

  /// Retargets from the presentation state, carrying the spring's velocity or a release's explicit velocity.
  ///
  /// - Parameters:
  ///   - index: The segment to choose.
  ///   - time: The clock time.
  ///   - reduced: Whether to crossfade rather than slide.
  ///   - velocity: A drag release's velocity; nil carries the existing spring's.
  mutating func choose(_ index: Int, at time: TimeInterval, reduced: Bool, velocity: CGFloat? = nil) {
    guard geometry.frames.indices.contains(index) else { return }
    let live = sample(at: time)
    let target = geometry.frames[index].midX
    selected = index
    if reduced {
      outgoing = velocity == nil ? frame(at: time) : nil
      fadeStart = time
      spring = nil
    } else {
      outgoing = nil
      spring = Motion.SegmentSpring(
        origin: live.position,
        velocity: velocity ?? live.velocity,
        target: target,
        start: time
      )
    }
    centre = target
  }

  /// Reconciles a measured layout or an external choice without replacing an in-flight spring with its target.
  ///
  /// - Parameters:
  ///   - frames: The segments in drawing order.
  ///   - index: The owner's selected index.
  ///   - time: The clock time.
  ///   - reduced: Whether Reduce Motion is on.
  mutating func update(frames: [CGRect], selected index: Int?, at time: TimeInterval, reduced: Bool) {
    let changed = frames != geometry.frames
    let wasEmpty = geometry.frames.isEmpty
    geometry.frames = frames
    guard let index, frames.indices.contains(index) else {
      cancel()
      selected = nil
      spring = nil
      outgoing = nil
      return
    }
    if wasEmpty {
      selected = index
      centre = frames[index].midX
    } else if !isPressed, selected != index || changed {
      choose(index, at: time, reduced: reduced)
    }
  }

  /// Grabs the live thumb immediately, retaining its velocity if the press turns out to be a tap.
  ///
  /// - Parameters:
  ///   - pointer: The down location.
  ///   - time: The clock time.
  mutating func press(at pointer: CGFloat, time: TimeInterval) {
    guard !isPressed, !geometry.frames.isEmpty else { return }
    let live = sample(at: time)
    let thumb = frame(at: time)
    grabCentre = thumb.contains(CGPoint(x: pointer, y: geometry.frames[0].midY)) ? live.position : pointer
    centre = live.position
    heldVelocity = live.velocity
    spring = nil
    isPressed = true
    isDragging = false
    startPointer = pointer
  }

  /// Tracks the pointer directly after the small movement threshold, previewing without committing a choice.
  ///
  /// - Parameters:
  ///   - pointer: The current pointer position.
  ///   - reduced: Whether to clamp rather than rubber-band.
  mutating func drag(to pointer: CGFloat, reduced: Bool) {
    guard isPressed else { return }
    if abs(pointer - startPointer) >= TabsLayout.dragThreshold { isDragging = true }
    guard isDragging else { return }
    rawCentre = grabCentre + pointer - startPointer
    centre = geometry.resisted(rawCentre, reduced: reduced).position
    spring = nil
    outgoing = nil
  }

  /// Ends this press exactly once, snapping to the nearest centre and passing the resisted pointer velocity on.
  ///
  /// - Parameters:
  ///   - pointer: The release location.
  ///   - velocity: The pointer's velocity in points per second.
  ///   - time: The clock time.
  ///   - reduced: Whether the release settles immediately.
  /// - Returns: The chosen index once, or nil without a press to release.
  mutating func release(at pointer: CGFloat, velocity: CGFloat, time: TimeInterval, reduced: Bool) -> Int? {
    guard isPressed else { return nil }
    drag(to: pointer, reduced: reduced)
    let dragged = isDragging
    let index = dragged ? geometry.nearest(to: centre) : geometry.segment(at: pointer)
    let carried = geometry.resisted(rawCentre, reduced: reduced).slope * velocity
    if let index { choose(index, at: time, reduced: reduced, velocity: dragged ? carried : nil) }
    cancel()
    return index
  }

  /// Clears pointer ownership after release, cancellation, disabling or disappearance.
  mutating func cancel() {
    isPressed = false
    isDragging = false
  }

  /// Stops sampling once all residual motion and any crossfade have ended.
  ///
  /// - Parameter time: The clock time.
  mutating func finish(at time: TimeInterval) {
    if let spring, time - spring.start >= Motion.SegmentSpring.lifetime { self.spring = nil }
    if fade(at: time) == 1 { outgoing = nil }
  }
}
