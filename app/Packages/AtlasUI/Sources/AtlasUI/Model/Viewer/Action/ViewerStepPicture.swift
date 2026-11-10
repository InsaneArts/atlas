/// The picture at the end of a step: the screen just before it, its mark, and for repeats folded into one step, how
/// many, which a stack and a count show.
public struct ViewerStepPicture: Equatable, Sendable {
  /// The picture before the step's first action: its name, shape and thumbnail once read.
  public var shot: ViewerActivityShot
  /// The mark over it; nil when nothing was recorded to place one.
  public var mark: ViewerActionMark?
  /// How many actions the step holds; 1 for one on its own.
  public var repeats: Int

  /// Creates a step's picture.
  ///
  /// - Parameters:
  ///   - shot: The picture before the step's first action.
  ///   - mark: The mark over it; none by default.
  ///   - repeats: How many actions the step holds; one by default.
  public init(shot: ViewerActivityShot, mark: ViewerActionMark? = nil, repeats: Int = 1) {
    self.shot = shot
    self.mark = mark
    self.repeats = repeats
  }
}
