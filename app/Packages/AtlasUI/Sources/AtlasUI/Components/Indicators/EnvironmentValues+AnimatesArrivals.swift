import SwiftUI

extension EnvironmentValues {
  /// Whether a view plays what it does as it arrives: a spinner waiting before it shows, a hero's disc popping and its
  /// check drawing. True in every window. The design renders turn it off, because a view drawn to an image is drawn
  /// once, before any animation runs, so each such view is drawn settled instead.
  @Entry public var animatesArrivals = true
}
